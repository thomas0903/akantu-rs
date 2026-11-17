// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_local_toolbox/
// neighborhood_base.{hh,cc} (createGrid, updatePairList, savePairs,
// saveNeighborCoords), neighborhood_base_inline_impl.hh
// (insertIntegrationPoint), non_local_neighborhood_base.{hh,cc} (`radius`
// parameter, registerNonLocalVariable, getRelevantGhostElements),
// non_local_neighborhood.hh, non_local_neighborhood_tmpl.hh
// (computeWeights, weightedAverageOnNeighbours, updateWeights,
// saveWeights).

//! A non-local neighbourhood (NL-002, NL-003): integration points in a
//! spatial grid, half pair lists in upstream's grid order, two weights per
//! pair, weighted averages.
//!
//! # Order of the pairs (upstream `updatePairList`)
//!
//! The grid has cells of size `1.2·R` centred on the mesh bounding-box
//! centre ([`akantu_core::spatial_grid::SpatialGrid`], cells in
//! lexicographic id order, items in insertion order). For every cell, for
//! every point `q1` of the cell in insertion order — stopping at the first
//! ghost point of the cell (upstream `break`: owned points are inserted
//! before ghosts) — for every neighbouring cell of `q1`'s cell (offsets in
//! `{−1, 0, 1}^d`, first coordinate fastest, the cell itself included), for
//! every point `q2` of that cell in insertion order: the pair `(q1, q2)` is
//! stored if `‖x1 − x2‖ ≤ R + tolerance` and `q2` is a ghost or
//! `q1 ≤ q2` ([`PointIdentity`]); owned pairs and ghost pairs go to two
//! lists. The distance is `sqrt(Σ_k (x1_k − x2_k)²)` summed in coordinate
//! order (Eigen's `norm()` of a ≤ 3-vector).
//!
//! # Weights (upstream `computeWeights`)
//!
//! For each list (owned, then ghost), for each pair in order:
//! `w0 = J(q2)·W(r, q1, q2)`, `V(q1) += w0`; if `q2` is owned and not `q1`
//! itself: `w1 = J(q1)·W(r, q2, q1)`, `V(q2) += w1`, else `w1 = 0`. Then for
//! each list, each pair: `w0 *= 1/V(q1)` and, if `q2` is owned,
//! `w1 *= 1/V(q2)` (multiplication by the reciprocal, as upstream). `J` is
//! the integration weight `det J · w` of the point, `V` the manager's
//! volumes (shared by all neighbourhoods).
//!
//! # Averages (upstream `weightedAverageOnNeighbours`)
//!
//! For each pair of a list, componentwise: `a(q1) += w0·v(q2)` and, for
//! owned pairs, `a(q2) += w1·v(q1)`; the owned list is averaged before the
//! ghost values arrive, the ghost list after.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use akantu_core::spatial_grid::SpatialGrid;
use akantu_core::{ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result};
use akantu_parser::param::{ParamInfo, ParamKind, ParamValue, ParameterAccess};
use akantu_parser::stream::format_double;
use akantu_parser::{ParameterRegistry, Parsable, ParserSection, ParserType};

use super::point::{PointIdentity, QuadPoint};
use super::weight::{PointRef, WeightFunction, WeightInternals};

/// Index of a ghost type in the two pair lists (upstream
/// `pair_list[ghost_type]`).
#[inline]
pub(crate) fn gi(g: GhostType) -> usize {
    match g {
        GhostType::NotGhost => 0,
        GhostType::Ghost => 1,
    }
}

/// `‖a − b‖` with upstream's operation order (sum of squares in coordinate
/// order, then `sqrt`).
#[inline]
#[must_use]
pub fn distance(a: &[Real], b: &[Real]) -> Real {
    let mut s = 0.;
    for (x, y) in a.iter().zip(b) {
        let d = x - y;
        s += d * d;
    }
    s.sqrt()
}

/// A neighbourhood (upstream `NonLocalNeighborhood<WeightFunction>`).
#[derive(Debug)]
pub struct NonLocalNeighborhood {
    id: String,
    parsable_id: String,
    radius: Real,
    spatial_dimension: usize,
    weight: Box<dyn WeightFunction>,
    grid: SpatialGrid<usize>,
    points: Vec<QuadPoint>,
    coords: Vec<Real>,
    /// Slot of each point in the manager's per-point layout (set by the
    /// manager at initialisation).
    pub(crate) slots: Vec<usize>,
    pair_list: [Vec<(usize, usize)>; 2],
    pair_weight: [Vec<[Real; 2]>; 2],
    weights_computed: bool,
    variables: BTreeSet<String>,
    /// [`super::SummationOrder::KeyOrder`] cache: per
    /// owned point, its neighbours (itself, owned and ghost) in key order,
    /// built with the pair lists; empty for ghost points.
    key_lists: Vec<Vec<usize>>,
    /// Per owned point, `W(r, q, q_j)·J_j` of every entry of its key list,
    /// computed with the pair weights (so the key-order route follows the
    /// `update_rate` schedule like upstream's route).
    key_weights: Vec<Vec<Real>>,
    /// Per pair list: every point's contributions to its weighted average in
    /// pair order (built on first use after the pair list changed), for the
    /// threaded [`NonLocalNeighborhood::weighted_average`].
    contributions: [std::sync::OnceLock<PointContributions>; 2],
    /// Inputs and volume outputs of the last weight computation, kept for
    /// weight functions whose weights depend on the distance only
    /// ([`WeightFunction::weights_depend_only_on_distance`]); dropped by
    /// every change of the points, pairs, radius or weight function.
    weight_memo: Option<Box<WeightMemo>>,
}

/// Pairs per part of the threaded weighted average.
const PAIRS_PER_PART: usize = 16_384;

/// Every point's contributions to its weighted average, in pair order:
/// `entries[offsets[p]..offsets[p + 1]]` are the codes `2k + side` of the
/// pairs `k` that add to point `p` (side 0: `p` is the pair's first point
/// and receives `w[0]·v(second)`; side 1: its second, `w[1]·v(first)`, only
/// in the not-ghost list), in increasing `k` and, within a pair whose two
/// points are `p`, side 0 first: the order of the serial loop's additions
/// to `p`'s slot.
#[derive(Debug, Default)]
struct PointContributions {
    offsets: Vec<u32>,
    entries: Vec<u32>,
}

impl PointContributions {
    fn build(list: &[(usize, usize)], nb_points: usize, ghost: GhostType) -> Self {
        let both = ghost != GhostType::Ghost;
        let mut count = vec![0u32; nb_points + 1];
        for &(i, j) in list {
            count[i + 1] += 1;
            if both {
                count[j + 1] += 1;
            }
        }
        for p in 0..nb_points {
            count[p + 1] += count[p];
        }
        let offsets = count.clone();
        let mut next = count;
        let mut entries = vec![0u32; offsets[nb_points] as usize];
        for (k, &(i, j)) in list.iter().enumerate() {
            let k = u32::try_from(k).unwrap_or(u32::MAX);
            entries[next[i] as usize] = 2 * k;
            next[i] += 1;
            if both {
                entries[next[j] as usize] = 2 * k + 1;
                next[j] += 1;
            }
        }
        Self { offsets, entries }
    }
}

/// What [`NonLocalNeighborhood::compute_weights`] read and produced besides
/// the pair weights (which stay stored): with the same inputs a new
/// computation gives the same bits, so it is skipped and the volumes are
/// set to the stored outputs.
#[derive(Clone, Debug, PartialEq)]
struct WeightMemo {
    radius: u64,
    identity: PointIdentity,
    key_order: bool,
    slots: Vec<usize>,
    /// `jacobians[slot]` per point, as bits.
    jacobians: Vec<u64>,
    /// `volumes[slot]` per point on entry, as bits.
    volumes_in: Vec<u64>,
    /// `volumes[slot]` per point on exit.
    volumes_out: Vec<Real>,
}

/// `values[slot]` of every slot, as bits (`None` if a slot is out of range).
fn slot_bits(values: &[Real], slots: &[usize]) -> Option<Vec<u64>> {
    slots
        .iter()
        .map(|&s| values.get(s).map(|v| v.to_bits()))
        .collect()
}

impl NonLocalNeighborhood {
    /// New neighbourhood `id` with the weight function `weight` (upstream
    /// constructor: `radius` defaults to 100, the weight function's
    /// parameters come from the `weight_parameter` sub-section).
    #[must_use]
    pub fn new(
        id: &str,
        parsable_id: &str,
        spatial_dimension: usize,
        weight: Box<dyn WeightFunction>,
    ) -> Self {
        Self {
            id: id.to_owned(),
            parsable_id: parsable_id.to_owned(),
            radius: 100.,
            spatial_dimension,
            weight,
            grid: SpatialGrid::new(spatial_dimension),
            points: Vec::new(),
            coords: Vec::new(),
            slots: Vec::new(),
            pair_list: [Vec::new(), Vec::new()],
            pair_weight: [Vec::new(), Vec::new()],
            weights_computed: false,
            variables: BTreeSet::new(),
            key_lists: Vec::new(),
            key_weights: Vec::new(),
            contributions: Default::default(),
            weight_memo: None,
        }
    }

    /// Upstream `initNeighborhood` → `createGrid`: spacing `1.2·R` in every
    /// direction, centred at `center` (the mesh bounding-box centre).
    ///
    /// # Errors
    /// `center` of the wrong length.
    pub fn init_neighborhood(&mut self, center: &[Real]) -> Result<()> {
        self.weight_memo = None;
        let safety_factor = 1.2;
        let spacing = vec![self.radius * safety_factor; self.spatial_dimension];
        self.grid = SpatialGrid::with_spacing(self.spatial_dimension, &spacing, center)?;
        Ok(())
    }

    /// Neighbourhood id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Radius (parameter `radius`).
    #[must_use]
    pub fn radius(&self) -> Real {
        self.radius
    }

    /// The weight function.
    #[must_use]
    pub fn weight_function(&self) -> &dyn WeightFunction {
        &*self.weight
    }

    /// The weight function, mutable (upstream access through the parser).
    pub fn weight_function_mut(&mut self) -> &mut dyn WeightFunction {
        self.weight_memo = None;
        &mut *self.weight
    }

    /// The grid.
    #[must_use]
    pub fn grid(&self) -> &SpatialGrid<usize> {
        &self.grid
    }

    /// Inserted points, in insertion order.
    #[must_use]
    pub fn points(&self) -> &[QuadPoint] {
        &self.points
    }

    /// Coordinates of point `i`.
    #[must_use]
    pub fn coords(&self, i: usize) -> &[Real] {
        let d = self.spatial_dimension;
        &self.coords[i * d..(i + 1) * d]
    }

    /// Point `i` with its layout slot (valid after the manager's
    /// initialisation).
    #[must_use]
    pub fn point_ref(&self, i: usize) -> PointRef<'_> {
        PointRef {
            point: &self.points[i],
            slot: self.slots.get(i).copied().unwrap_or(usize::MAX),
        }
    }

    /// Variables averaged in this neighbourhood (non-local names).
    #[must_use]
    pub fn non_local_variables(&self) -> &BTreeSet<String> {
        &self.variables
    }

    /// Upstream `registerNonLocalVariable(id)`.
    pub fn register_non_local_variable(&mut self, nl_name: &str) {
        self.variables.insert(nl_name.to_owned());
    }

    /// Upstream `insertIntegrationPoint(q, coords)`.
    ///
    /// # Errors
    /// Coordinates of the wrong length, non-finite or outside the grid's
    /// representable range (upstream: undefined, UD-270).
    pub fn insert_integration_point(&mut self, q: QuadPoint, coords: &[Real]) -> Result<()> {
        if coords.len() != self.spatial_dimension {
            return Err(Error::DimensionMismatch {
                context: "NonLocalNeighborhood::insert_integration_point",
                expected: self.spatial_dimension,
                got: coords.len(),
            });
        }
        self.weight_memo = None;
        let index = self.points.len();
        self.grid.insert(index, coords)?;
        self.points.push(q);
        self.coords.extend_from_slice(coords);
        Ok(())
    }

    /// Drop the points, pairs and weights (rebuild), keep the grid
    /// geometry, the weight function and the variables.
    pub(crate) fn clear_points(&mut self) -> Result<()> {
        self.weight_memo = None;
        let spacing = self.grid.spacing().to_vec();
        let center = self.grid.center().to_vec();
        self.grid = SpatialGrid::with_spacing(self.spatial_dimension, &spacing, &center)?;
        self.points.clear();
        self.coords.clear();
        self.slots.clear();
        self.pair_list = [Vec::new(), Vec::new()];
        self.contributions = Default::default();
        self.pair_weight = [Vec::new(), Vec::new()];
        self.weights_computed = false;
        Ok(())
    }

    /// The pairs of a list (upstream `getPairLists(ghost_type)`): point
    /// indices `(q1, q2)`.
    #[must_use]
    pub fn pair_list(&self, ghost: GhostType) -> &[(usize, usize)] {
        &self.pair_list[gi(ghost)]
    }

    /// The pairs of a list as points.
    pub fn pairs(&self, ghost: GhostType) -> impl Iterator<Item = (&QuadPoint, &QuadPoint)> + '_ {
        self.pair_list[gi(ghost)]
            .iter()
            .map(|&(i, j)| (&self.points[i], &self.points[j]))
    }

    /// The two weights of every pair of a list (`[w(q1←q2), w(q2←q1)]`),
    /// empty before the first weight computation.
    #[must_use]
    pub fn pair_weights(&self, ghost: GhostType) -> &[[Real; 2]] {
        &self.pair_weight[gi(ghost)]
    }

    /// Upstream `updatePairList` (see the module docs for the order).
    ///
    /// # Errors
    /// A point whose cell cannot be computed (non-finite coordinates).
    pub fn update_pair_list(&mut self, identity: PointIdentity, tolerance: Real) -> Result<()> {
        self.weight_memo = None;
        let reach = self.radius + tolerance;
        let mut lists: [Vec<(usize, usize)>; 2] = [Vec::new(), Vec::new()];
        for (_, cell) in self.grid.cells() {
            for &i in cell {
                let q1 = &self.points[i];
                if q1.ghost == GhostType::Ghost {
                    break;
                }
                let x1 = self.coords(i);
                let cell_id = self.grid.cell_id(x1)?;
                for neighbor in cell_id.neighbors() {
                    for &j in self.grid.cell(&neighbor) {
                        let q2 = &self.points[j];
                        let d = distance(x1, self.coords(j));
                        if d <= reach
                            && (q2.ghost == GhostType::Ghost || identity.half_list_le(q1, q2))
                        {
                            lists[gi(q2.ghost)].push((i, j));
                        }
                    }
                }
            }
        }
        self.pair_list = lists;
        self.contributions = Default::default();
        self.pair_weight = [Vec::new(), Vec::new()];
        self.weights_computed = false;
        self.key_lists = self.build_key_lists();
        self.key_weights = Vec::new();
        Ok(())
    }

    /// Key-ordered neighbour lists of the owned points (itself, owned and
    /// ghost neighbours; sorted by `(key, ghost)`, duplicates removed).
    fn build_key_lists(&self) -> Vec<Vec<usize>> {
        let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); self.points.len()];
        for (g, list) in self.pair_list.iter().enumerate() {
            for &(i, j) in list {
                neighbours[i].push(j);
                if g == 0 && i != j {
                    neighbours[j].push(i);
                }
            }
        }
        for (i, nb) in neighbours.iter_mut().enumerate() {
            if self.points[i].ghost == GhostType::Ghost {
                nb.clear();
                continue;
            }
            nb.sort_by_key(|&j| (self.points[j].key(), self.points[j].ghost as u8));
            nb.dedup();
        }
        neighbours
    }

    /// Upstream `NeighborhoodBase::onElementsRemoved` (the
    /// cleanup of the ghost elements in no pair): every ghost point of a
    /// renumbered block takes its element's new index and `global_num =
    /// element·nb_quad + num_point`; points of removed elements leave the
    /// neighbourhood (an error if a pair holds one: upstream's assertion
    /// "has been removed instead of just being renumbered"). The kept points
    /// keep their order, the grid is rebuilt by inserting them again in that
    /// order (same cells, same per-cell order minus the removed points;
    /// upstream leaves stale entries in its grid, read only by the dump
    /// helpers), the pair lists keep their order with renumbered indices.
    ///
    /// # Errors
    /// See above; grid errors.
    pub(crate) fn renumber_ghost_points(
        &mut self,
        new_numbering: &ElementTypeMapArray<Idx>,
        nb_quad: &dyn Fn(ElementType) -> Result<usize>,
    ) -> Result<()> {
        self.weight_memo = None;
        let n_old = self.points.len();
        let mut new_index = vec![usize::MAX; n_old];
        let mut points = Vec::with_capacity(n_old);
        let mut coords = Vec::with_capacity(self.coords.len());
        let d = self.spatial_dimension;
        for (i, q) in self.points.iter().enumerate() {
            let mut q = *q;
            if q.ghost == GhostType::Ghost
                && let Ok(num) = new_numbering.get(q.ty, GhostType::Ghost)
            {
                let k = usize::try_from(q.element)
                    .ok()
                    .and_then(|k| num.as_slice().get(k).copied())
                    .ok_or_else(|| {
                        Error::invalid(format!(
                            "neighbourhood {}: the removal numbering has no entry for {q}",
                            self.id
                        ))
                    })?;
                if k < 0 {
                    continue;
                }
                let nq = Idx::try_from(nb_quad(q.ty)?)
                    .map_err(|_| Error::invalid("quadrature points per element"))?;
                q.element = k;
                q.global_num = k * nq + q.num_point;
                q.global_element = akantu_core::GlobalIdx::from(k);
            }
            new_index[i] = points.len();
            points.push(q);
            coords.extend_from_slice(&self.coords[i * d..(i + 1) * d]);
        }
        let mut lists: [Vec<(usize, usize)>; 2] = [Vec::new(), Vec::new()];
        for (g, list) in self.pair_list.iter().enumerate() {
            for &(a, b) in list {
                let (na, nb) = (new_index[a], new_index[b]);
                if na == usize::MAX || nb == usize::MAX {
                    let q = if na == usize::MAX {
                        self.points[a]
                    } else {
                        self.points[b]
                    };
                    return Err(Error::invalid(format!(
                        "A local quadrature_point {} as been removed instead of just being \
                         renumbered: {}",
                        q.upstream_string(),
                        self.parsable_id
                    )));
                }
                lists[g].push((na, nb));
            }
        }
        let spacing = self.grid.spacing().to_vec();
        let center = self.grid.center().to_vec();
        let mut grid = SpatialGrid::with_spacing(d, &spacing, &center)?;
        for (i, _) in points.iter().enumerate() {
            grid.insert(i, &coords[i * d..(i + 1) * d])?;
        }
        self.grid = grid;
        self.points = points;
        self.coords = coords;
        self.slots.clear();
        self.pair_list = lists;
        self.contributions = Default::default();
        self.pair_weight = [Vec::new(), Vec::new()];
        self.weights_computed = false;
        self.key_lists = self.build_key_lists();
        self.key_weights = Vec::new();
        Ok(())
    }

    /// Ghost points appearing in a pair (upstream
    /// `getRelevantGhostElements`, by point).
    #[must_use]
    pub fn relevant_ghost_points(&self) -> BTreeSet<usize> {
        let mut s = BTreeSet::new();
        for list in &self.pair_list {
            for &(i, j) in list {
                for k in [i, j] {
                    if self.points[k].ghost == GhostType::Ghost {
                        s.insert(k);
                    }
                }
            }
        }
        s
    }

    /// `true` once weights were computed for the current pair lists.
    #[must_use]
    pub fn weights_computed(&self) -> bool {
        self.weights_computed
    }

    /// Upstream `NonLocalNeighborhood::computeWeights` (module docs).
    /// `jacobians` and `volumes` are the manager's per-slot arrays.
    ///
    /// # Errors
    /// The weight function's `update_internals` errors.
    pub(crate) fn compute_weights(
        &mut self,
        identity: PointIdentity,
        key_order: bool,
        jacobians: &[Real],
        volumes: &mut [Real],
        internals: &WeightInternals,
    ) -> Result<()> {
        self.weight.set_radius(self.radius);
        self.weight.update_internals(internals)?;
        // the same inputs as the stored computation: the same weights and
        // volumes (the pair and key weights are still stored)
        let memo_inputs = if self.weight.weights_depend_only_on_distance()
            && self.slots.len() == self.points.len()
        {
            slot_bits(jacobians, &self.slots).zip(slot_bits(volumes, &self.slots))
        } else {
            None
        };
        if let (Some(m), Some((jac, vin))) = (&self.weight_memo, &memo_inputs)
            && self.weights_computed
            && m.radius == self.radius.to_bits()
            && m.identity == identity
            && m.key_order == key_order
            && m.slots == self.slots
            && &m.jacobians == jac
            && &m.volumes_in == vin
        {
            for (&s, &v) in self.slots.iter().zip(&m.volumes_out) {
                volumes[s] = v;
            }
            return Ok(());
        }
        self.weight_memo = None;
        for g in 0..2 {
            let mut weights = vec![[0.; 2]; self.pair_list[g].len()];
            for (w, &(i, j)) in weights.iter_mut().zip(&self.pair_list[g]) {
                let (p1, p2) = (self.point_ref(i), self.point_ref(j));
                let (q1, q2) = (p1.point, p2.point);
                let (s1, s2) = (p1.slot, p2.slot);
                let r = distance(self.coords(i), self.coords(j));
                w[0] = jacobians[s2] * self.weight.weight(r, p1, p2);
                volumes[s1] += w[0];
                if q2.ghost != GhostType::Ghost && !identity.is_self_pair(q1, q2) {
                    w[1] = jacobians[s1] * self.weight.weight(r, p2, p1);
                    volumes[s2] += w[1];
                } else {
                    w[1] = 0.;
                }
            }
            self.pair_weight[g] = weights;
        }
        for g in 0..2 {
            for (w, &(i, j)) in self.pair_weight[g].iter_mut().zip(&self.pair_list[g]) {
                w[0] *= 1. / volumes[self.slots[i]];
                if self.points[j].ghost != GhostType::Ghost {
                    w[1] *= 1. / volumes[self.slots[j]];
                }
            }
        }
        if key_order {
            self.key_weights = self
                .key_lists
                .iter()
                .enumerate()
                .map(|(i, nb)| {
                    nb.iter()
                        .map(|&j| {
                            let r = distance(self.coords(i), self.coords(j));
                            self.weight.weight(r, self.point_ref(i), self.point_ref(j))
                                * jacobians[self.slots[j]]
                        })
                        .collect()
                })
                .collect();
        }
        self.weights_computed = true;
        if let Some((jac, vin)) = memo_inputs {
            self.weight_memo = Some(Box::new(WeightMemo {
                radius: self.radius.to_bits(),
                identity,
                key_order,
                slots: self.slots.clone(),
                jacobians: jac,
                volumes_in: vin,
                volumes_out: self.slots.iter().map(|&s| volumes[s]).collect(),
            }));
        }
        Ok(())
    }

    /// Upstream `weightedAverageOnNeighbours(to_accumulate, accumulated,
    /// nb_component, ghost_type)` over per-slot arrays of `nb_component`
    /// values.
    ///
    /// With more than one part of `exec`, and when the slots increase with
    /// the points (the manager's layout), every point's slot receives its
    /// contributions in pair order from the point's own list
    /// ([`PointContributions`]): the additions of the serial loop to that
    /// slot, in their order, so the same bits for every thread count; the
    /// points split over the threads. Otherwise the serial loop.
    pub(crate) fn weighted_average(
        &self,
        exec: &akantu_core::exec::Exec,
        to_accumulate: &[Real],
        accumulated: &mut [Real],
        nb_component: usize,
        ghost: GhostType,
    ) {
        let g = gi(ghost);
        let n = nb_component;
        let np = self.points.len();
        let threaded = n > 0
            && self.pair_list[g].len() < (u32::MAX / 2) as usize
            && exec.nb_parts(self.pair_list[g].len(), PAIRS_PER_PART) > 1
            && self.slots.len() == np
            && self.slots.windows(2).all(|w| w[0] < w[1])
            && self.slots.last().is_some_and(|&s| {
                (s + 1) * n <= accumulated.len() && (s + 1) * n <= to_accumulate.len()
            })
            && self.pair_weight[g].len() == self.pair_list[g].len();
        if threaded {
            let table = self.contributions[g]
                .get_or_init(|| PointContributions::build(&self.pair_list[g], np, ghost));
            let parts = exec.nb_parts(self.pair_list[g].len(), PAIRS_PER_PART);
            // point boundaries with balanced numbers of contributions, then
            // the slot ranges they own in `accumulated` (contiguous: the
            // slots increase with the points)
            let total = table.entries.len();
            let mut points = vec![0usize];
            for p in 1..parts {
                let target = u32::try_from(p * total / parts).unwrap_or(u32::MAX);
                let k = table.offsets.partition_point(|&o| o < target).min(np);
                points.push(k.max(*points.last().unwrap_or(&0)));
            }
            points.push(np);
            let mut bounds: Vec<usize> = points
                .iter()
                .map(|&k| {
                    if k < np {
                        self.slots[k]
                    } else {
                        accumulated.len() / n
                    }
                })
                .collect();
            bounds[0] = 0;
            let len = accumulated.len() / n;
            let acc = &mut accumulated[..len * n];
            let (slots, list, weights) = (&self.slots, &self.pair_list[g], &self.pair_weight[g]);
            exec.for_each_bounds(
                &bounds,
                akantu_core::exec::Tuples::new(acc, n),
                |first, mut part| {
                    let part = part.as_mut_slice();
                    let end = first + part.len() / n;
                    // the points whose slots are in [first, end)
                    let p0 = slots.partition_point(|&s| s < first);
                    let p1 = slots.partition_point(|&s| s < end);
                    for p in p0..p1 {
                        let dst = (slots[p] - first) * n;
                        let range = table.offsets[p] as usize..table.offsets[p + 1] as usize;
                        for &code in &table.entries[range] {
                            let k = (code >> 1) as usize;
                            let (i, j) = list[k];
                            let (w, src) = if code & 1 == 0 {
                                (weights[k][0], slots[j] * n)
                            } else {
                                (weights[k][1], slots[i] * n)
                            };
                            for c in 0..n {
                                part[dst + c] += w * to_accumulate[src + c];
                            }
                        }
                    }
                },
            );
            return;
        }
        for (w, &(i, j)) in self.pair_weight[g].iter().zip(&self.pair_list[g]) {
            let (s1, s2) = (self.slots[i] * n, self.slots[j] * n);
            for c in 0..n {
                accumulated[s1 + c] += w[0] * to_accumulate[s2 + c];
            }
            if ghost != GhostType::Ghost {
                for c in 0..n {
                    accumulated[s2 + c] += w[1] * to_accumulate[s1 + c];
                }
            }
        }
    }

    /// The partition-independent route ([`super::SummationOrder::KeyOrder`]):
    /// for every owned point, `Σ_j W(r, q, q_j)·J_j·v_j / Σ_j W(r, q, q_j)·J_j`
    /// over its neighbours (itself, owned and ghost) in key order; the
    /// point's own value when the weight sum is not positive. The neighbour
    /// lists are cached per rebuild and the products `W·J` per weight update.
    pub(crate) fn key_order_average(
        &self,
        to_accumulate: &[Real],
        accumulated: &mut [Real],
        nb_component: usize,
    ) {
        let n = nb_component;
        let mut num = vec![0.; n];
        for (i, (nb, ws)) in self.key_lists.iter().zip(&self.key_weights).enumerate() {
            if self.points[i].ghost == GhostType::Ghost {
                continue;
            }
            num.iter_mut().for_each(|v| *v = 0.);
            let mut den = 0.;
            for (&j, &w) in nb.iter().zip(ws) {
                for (c, v) in num.iter_mut().enumerate() {
                    *v += w * to_accumulate[self.slots[j] * n + c];
                }
                den += w;
            }
            let si = self.slots[i] * n;
            for c in 0..n {
                accumulated[si + c] = if den > 0. {
                    num[c] / den
                } else {
                    to_accumulate[si + c]
                };
            }
        }
    }

    /// Upstream `savePairs`: one line `"<q1> <q2> "` per pair (owned list,
    /// then ghost list), points printed as upstream's
    /// `std::to_string(IntegrationPoint)`. (Upstream also writes the pairs
    /// as a segment mesh `<filename>.msh` in serial runs:
    /// [`NonLocalNeighborhood::pairs_mesh_arrays`].)
    #[must_use]
    pub fn save_pairs_text(&self) -> String {
        let mut s = String::new();
        for list in &self.pair_list {
            for &(i, j) in list {
                let _ = writeln!(
                    s,
                    "{} {} ",
                    self.points[i].upstream_string(),
                    self.points[j].upstream_string()
                );
            }
        }
        s
    }

    /// The segment mesh of upstream `savePairs` (`_segment_2` elements,
    /// mesh data `tag_1`): nodes are the points in order of first
    /// appearance (coordinates), each pair `(q1, q2)` gives the segment
    /// `(n1, n2)` tagged `n1 + 1` and, if `n1 ≠ n2`, the segment `(n2, n1)`
    /// tagged `n2 + 1`. Returns `(nodes (dim per node), connectivity,
    /// tags)`.
    #[must_use]
    pub fn pairs_mesh_arrays(&self) -> (Vec<Real>, Vec<[usize; 2]>, Vec<i32>) {
        let mut node_of = std::collections::BTreeMap::new();
        let mut nodes = Vec::new();
        for list in &self.pair_list {
            for &(i, j) in list {
                for k in [i, j] {
                    let key = (
                        self.points[k].ty,
                        self.points[k].element,
                        self.points[k].ghost as u8,
                        self.points[k].num_point,
                    );
                    if let std::collections::btree_map::Entry::Vacant(e) = node_of.entry(key) {
                        e.insert(nodes.len() / self.spatial_dimension.max(1));
                        nodes.extend_from_slice(self.coords(k));
                    }
                }
            }
        }
        let node = |k: usize| {
            let p = &self.points[k];
            node_of[&(p.ty, p.element, p.ghost as u8, p.num_point)]
        };
        let (mut conn, mut tags) = (Vec::new(), Vec::new());
        for list in &self.pair_list {
            for &(i, j) in list {
                let (n1, n2) = (node(i), node(j));
                conn.push([n1, n2]);
                tags.push(i32::try_from(n1 + 1).unwrap_or(i32::MAX));
                if n1 != n2 {
                    conn.push([n2, n1]);
                    tags.push(i32::try_from(n2 + 1).unwrap_or(i32::MAX));
                }
            }
        }
        (nodes, conn, tags)
    }

    /// Upstream `saveNeighborCoords`: for every point in grid order,
    /// `#neighbors for quad <global_num>`, its coordinates, then the
    /// coordinates of every other point it is paired with (owned list, then
    /// ghost list, in pair order); coordinates as Akantu prints a column
    /// vector (`[[x], [y]]`, default stream precision).
    #[must_use]
    pub fn save_neighbor_coords_text(&self) -> String {
        let fmt = |c: &[Real]| {
            let parts: Vec<String> = c
                .iter()
                .map(|x| format!("[{}]", format_double(*x)))
                .collect();
            format!("[{}]", parts.join(", "))
        };
        let mut s = String::new();
        for (_, cell) in self.grid.cells() {
            for &i in cell {
                let q1 = &self.points[i];
                let _ = writeln!(s, "#neighbors for quad {}", q1.global_num);
                let _ = writeln!(s, "{}", fmt(self.coords(i)));
                for list in &self.pair_list {
                    for &(a, b) in list {
                        let (pa, pb) = (&self.points[a], &self.points[b]);
                        let other = if q1.same_point(pa) && !pb.same_point(q1) {
                            b
                        } else if q1.same_point(pb) && !pa.same_point(q1) {
                            a
                        } else {
                            continue;
                        };
                        let _ = writeln!(s, "{}", fmt(self.coords(other)));
                    }
                }
            }
        }
        s
    }

    /// Upstream `saveWeights`: `w1: <w0> w2: <w1>` per pair (owned list,
    /// then ghost list), default stream precision.
    ///
    /// # Errors
    /// Weights not computed yet (upstream: debug assertion).
    pub fn save_weights_text(&self) -> Result<String> {
        if !self.weights_computed {
            return Err(Error::invalid(format!(
                "the weights of the neighborhood {} have not been computed yet",
                self.id
            )));
        }
        let mut s = String::new();
        for list in &self.pair_weight {
            for w in list {
                let _ = writeln!(s, "w1: {} w2: {}", format_double(w[0]), format_double(w[1]));
            }
        }
        Ok(s)
    }
}

impl ParameterRegistry for NonLocalNeighborhood {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        match name {
            "radius" => Ok(ParamValue::Real(self.radius)),
            _ => Err(Error::not_found("parameter", name)),
        }
    }
    fn set_param(&mut self, name: &str, _value: ParamValue) -> Result<()> {
        match name {
            "radius" => Err(Error::invalid("Parameter radius is not writable")),
            _ => Err(Error::not_found("parameter", name)),
        }
    }
    fn param_names(&self) -> Vec<&'static str> {
        vec!["radius"]
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        (name == "radius").then_some(ParamInfo {
            kind: ParamKind::Real,
            access: ParameterAccess::PARSABLE | ParameterAccess::READABLE,
        })
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.weight_memo = None;
        match (name, value) {
            ("radius", ParamValue::Real(r)) => {
                self.radius = r;
                Ok(())
            }
            ("radius", ParamValue::Int(i)) => {
                #[allow(clippy::cast_precision_loss)]
                // upstream Real parameter read from an integer literal
                {
                    self.radius = i as Real;
                }
                Ok(())
            }
            ("radius", other) => Err(Error::invalid(format!(
                "radius expects a Real, got {other:?}"
            ))),
            _ => Err(Error::not_found("parameter", name)),
        }
    }
}

impl Parsable for NonLocalNeighborhood {
    fn section_type(&self) -> ParserType {
        ParserType::NonLocal
    }
    fn parsable_id(&self) -> &str {
        &self.parsable_id
    }
    /// Upstream `registerSubSection(_weight_function, "weight_parameter",
    /// weight_function)`.
    fn parse_sub_section(&mut self, section: &ParserSection, permissive: bool) -> Result<()> {
        self.weight_memo = None;
        if section.section_type == ParserType::WeightFunction && section.name == "weight_parameter"
        {
            return self.weight.parse_section(section, permissive);
        }
        let msg = format!(
            "No parsable defined for sub sections of type <{},{}> in {}",
            section.section_type, section.name, self.parsable_id
        );
        if permissive {
            log::warn!("{msg}");
            Ok(())
        } else {
            Err(Error::invalid(msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::non_local::weight::BaseWeightFunction;

    /// A `base_wf` neighbourhood on a 2-D grid of points with slots.
    fn grid_neighborhood() -> NonLocalNeighborhood {
        let mut n = NonLocalNeighborhood::new(
            "nl",
            "non_local:nl",
            2,
            Box::new(BaseWeightFunction::default()),
        );
        n.set_param_from_input("radius", ParamValue::Real(0.35))
            .unwrap();
        n.init_neighborhood(&[0.5, 0.5]).unwrap();
        let mut k = 0;
        for i in 0..6 {
            for j in 0..6 {
                let q = QuadPoint::new(ElementType::Triangle3, GhostType::NotGhost, k, 0, 1);
                let x = [0.1 + 0.17 * f64::from(i), 0.05 + 0.18 * f64::from(j)];
                n.insert_integration_point(q, &x).unwrap();
                k += 1;
            }
        }
        n.update_pair_list(PointIdentity::Complete, 1e-12).unwrap();
        n.slots = (0..n.points().len()).map(|i| 2 * i + 1).collect();
        n
    }

    /// The threaded weighted average (per-point contribution lists in pair
    /// order) equals the serial loop bit for bit at 1 to 8 threads, finest
    /// and automatic grain, for both pair lists, with self-pairs and points
    /// that appear in many pairs; with slots that do not increase the
    /// serial loop runs.
    #[test]
    fn threaded_weighted_average_equals_the_serial_loop() {
        use akantu_core::exec::{Exec, ExecPolicy, Grain};
        let mut n = grid_neighborhood();
        let np = n.points().len();
        let mut x: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        // a random pair list over the points (self-pairs included) with
        // random weights of every magnitude
        for list in &mut n.pair_list {
            *list = (0..5000)
                .map(|_| ((next() % np as u64) as usize, (next() % np as u64) as usize))
                .collect();
            list.push((3, 3));
        }
        for w in &mut n.pair_weight {
            *w = (0..5001)
                .map(|_| {
                    [
                        f64::from_bits(0x3ff0_0000_0000_0000 | (next() >> 12)) - 1.5,
                        f64::from_bits(0x3fe0_0000_0000_0000 | (next() >> 12)) * 3.0,
                    ]
                })
                .collect();
        }
        n.contributions = Default::default();
        let nb = 2;
        let len = (2 * np + 2) * nb;
        let v: Vec<Real> = (0..len).map(|k| (k as Real * 0.37).sin() * 1e3).collect();
        for slots in [
            (0..np).map(|i| 2 * i + 1).collect::<Vec<_>>(),
            (0..np).map(|i| 2 * (np - 1 - i) + 1).collect(),
        ] {
            n.slots = slots;
            for ghost in GhostType::ALL {
                let mut reference: Vec<Real> = (0..len).map(|k| k as Real * 1e-3).collect();
                let start = reference.clone();
                n.weighted_average(Exec::serial_ref(), &v, &mut reference, nb, ghost);
                for threads in [1, 2, 3, 4, 8] {
                    for grain in [Grain::Finest, Grain::Auto] {
                        let exec = Exec::new(
                            ExecPolicy::serial()
                                .with_threads(threads)
                                .unwrap()
                                .with_grain(grain),
                        )
                        .unwrap();
                        let mut out = start.clone();
                        n.weighted_average(&exec, &v, &mut out, nb, ghost);
                        let same = out
                            .iter()
                            .zip(&reference)
                            .all(|(a, b)| a.to_bits() == b.to_bits());
                        assert!(same, "{ghost:?} threads {threads} {grain:?}");
                    }
                }
            }
        }
    }

    /// The stored weights of a distance-only weight function are reused
    /// while the inputs are unchanged and give the bits of a computation
    /// from scratch, also when the incoming volumes change (another
    /// neighbourhood's contributions to shared slots).
    #[test]
    fn stored_weights_equal_recomputed_weights() {
        let mut a = grid_neighborhood();
        let mut b = grid_neighborhood();
        let np = a.points().len();
        let nslots = 2 * np + 2;
        let jac: Vec<Real> = (0..nslots).map(|s| 0.5 + 0.01 * s as Real).collect();
        let mut jac2 = jac.clone();
        jac2[3] = 0.75; // the integration weight of point 1
        let internals = WeightInternals::new();
        let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        let mut stored_ptr = None;
        // rounds: 0 first computation, 1-2 reuse, 3 other incoming volumes
        // (another neighbourhood's contributions to shared slots), 4 back,
        // 5 changed Jacobians, 6 reuse, 7 changed radius, 8 reuse, 9 changed
        // slots, 10 reuse
        for round in 0..11 {
            if round == 7 {
                for n in [&mut a, &mut b] {
                    n.set_param_from_input("radius", ParamValue::Real(0.3))
                        .unwrap();
                }
            }
            if round == 9 {
                for n in [&mut a, &mut b] {
                    n.slots = (0..np).map(|i| 2 * (np - 1 - i) + 1).collect();
                }
            }
            let jac_r = if round >= 5 { &jac2 } else { &jac };
            let mut va = vec![0.; nslots];
            if round == 3 {
                va[3] = 0.25;
            }
            let mut vb = va.clone();
            a.compute_weights(PointIdentity::Complete, true, jac_r, &mut va, &internals)
                .unwrap();
            let _ = b.weight_function_mut(); // drops b's stored weights
            b.compute_weights(PointIdentity::Complete, true, jac_r, &mut vb, &internals)
                .unwrap();
            assert_eq!(bits(&va), bits(&vb), "round {round}");
            for g in [GhostType::NotGhost, GhostType::Ghost] {
                let (wa, wb) = (a.pair_weights(g), b.pair_weights(g));
                assert_eq!(wa.len(), wb.len());
                for (x, y) in wa.iter().zip(wb) {
                    assert_eq!(
                        [x[0].to_bits(), x[1].to_bits()],
                        [y[0].to_bits(), y[1].to_bits()],
                        "round {round}"
                    );
                }
            }
            assert_eq!(a.key_weights.len(), b.key_weights.len());
            for (x, y) in a.key_weights.iter().zip(&b.key_weights) {
                assert_eq!(bits(x), bits(y), "round {round}");
            }
            // the stored weights are reused exactly when nothing changed
            let ptr = a.pair_weights(GhostType::NotGhost).as_ptr();
            if matches!(round, 1 | 2 | 6 | 8 | 10) {
                assert_eq!(Some(ptr), stored_ptr, "reused in round {round}");
            }
            stored_ptr = Some(ptr);
        }
        assert!(!a.pair_weights(GhostType::NotGhost).is_empty());
    }
}
