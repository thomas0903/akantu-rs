// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_local_toolbox/
// non_local_manager.cc (createNeighborhoodSynchronizers: the agreement of
// the neighbourhood names and the dummy grid; synchronize;
// cleanupExtraGhostElements; onElementsRemoved), non_local_neighborhood_base.cc
// (createGridSynchronizer, synchronize, getRelevantGhostElements),
// non_local_neighborhood_tmpl.hh (getNbData / packData / unpackData of
// _mnl_for_average and _mnl_weight), neighborhood_base.cc
// (onElementsRemoved), weight_functions/remove_damaged_weight_function_inline_impl.hh
// (packData of _mnl_weight).

//! The distributed ghost machinery of the non-local manager (MPI-048,
//! MPI-049): one grid synchroniser per neighbourhood and the
//! exchanges `_mnl_for_average` / `_mnl_weight` at the ghost slots.
//!
//! The grid synchronisers themselves are built by the model owning the laws
//! (a mesh mutation: new pure-ghost nodes and ghost elements dispatched to
//! the model's consumers, `akantu_mesh::grid_synchronizer::
//! create_grid_synchronizer_shared`) from [`NeighborhoodGridCells`], the
//! view of a neighbourhood's spatial grid as a grid of elements (upstream
//! slices its `IntegrationPoint`s into `Element`s in a `std::set<Element>`,
//! `grid_synchronizer.cc:117-123`). [`GridGhosts`] keeps them, one per
//! **global** neighbourhood name (upstream `global_neighborhoods`: the
//! union of every rank's names, agreed with [`agree_neighborhood_names_in`];
//! a name without a local neighbourhood gets an empty synchroniser, the
//! port's upstream dummy synchroniser on a dummy grid whose box intersects
//! no rank, [`EmptyGridCells`]).
//!
//! **The dummy path is unreachable from the solid model**:
//! `SolidMechanicsModel` registers the neighbourhood of every
//! non-local law on every rank (upstream
//! `insertIntegrationPointsInNeighborhoods`; the laws come from the same
//! input on every rank, also on a rank that holds none of a law's
//! elements), so every global name has a local neighbourhood. It is
//! upstream's defensive code (`non_local_manager.cc:216-231`), kept for a
//! program whose ranks register different neighbourhoods; only
//! [`EmptyGridCells`] has a unit test.
//!
//! # Exchanges (upstream `NonLocalNeighborhood::{getNbData, packData,
//! unpackData}`)
//!
//! For one neighbourhood and one tag, every element of a send scheme packs,
//! field after field (upstream: one `packElementalDataHelper` per
//! variable), the values of every integration point of the element at the
//! manager's slots (`nb_component` values per point); the receiver unpacks
//! them into its ghost slots. `_mnl_for_average` carries the flattened local
//! variables the neighbourhood averages (upstream: the non-local
//! variables registered in the neighbourhood, by name, each packing its
//! local variable), `_mnl_weight` the weight-function internals (upstream:
//! `RemoveDamagedWeightFunction::packData` packs `damage`; see UD-1103 for
//! `damage_wf`). Every rank runs one exchange per global name and tag, in
//! name order, empty schemes included (the port's exchanges are collective
//! scopes: a rank-local failure before an exchange is agreed on every rank
//! at that exchange, `akantu_comm::phase`).
//!
//! Upstream starts `_mnl_for_average` before the weight update and
//! completes it after the owned pairs are averaged; the values packed and
//! unpacked are the same when the exchange runs at the completion point
//! (the owned slots do not change in between), which is what
//! [`GridGhostExchange`] does — upstream's own synchronous `_mnl_weight`
//! exchanges of the weight update run before it on every rank.

use std::collections::{BTreeMap, BTreeSet};

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::Phases;
use akantu_comm::{DataAccessor, Synchronizer};
use akantu_core::enums::SynchronizationTag;
use akantu_core::spatial_grid::CellId;
use akantu_core::{Element, ElementType, Error, Idx, Real, Result};
use akantu_mesh::grid_synchronizer::GridCells;

use super::manager::{NonLocalGhostExchange, NonLocalTag, PointLayout};
use super::neighborhood::NonLocalNeighborhood;

/// Bytes of one `Real`.
const R: usize = std::mem::size_of::<Real>();

/// A neighbourhood's spatial grid seen as a grid of elements (upstream
/// `GridSynchronizer::createGridSynchronizer<IntegrationPoint>`: the cells
/// hold integration points, `to_send` keeps their elements).
#[derive(Clone, Copy, Debug)]
pub struct NeighborhoodGridCells<'a> {
    nbh: &'a NonLocalNeighborhood,
}

impl<'a> NeighborhoodGridCells<'a> {
    /// The grid view of `nbh`.
    #[must_use]
    pub fn new(nbh: &'a NonLocalNeighborhood) -> Self {
        Self { nbh }
    }
}

impl GridCells for NeighborhoodGridCells<'_> {
    fn dimension(&self) -> usize {
        self.nbh.grid().dimension()
    }

    fn lower_bounds(&self) -> &[Real] {
        self.nbh.grid().lower_bounds()
    }

    fn upper_bounds(&self) -> &[Real] {
        self.nbh.grid().upper_bounds()
    }

    fn spacing(&self) -> &[Real] {
        self.nbh.grid().spacing()
    }

    fn cell_index(&self, position: Real, direction: usize) -> Result<Idx> {
        self.nbh.grid().cell_index(position, direction)
    }

    fn elements_in_cell_range(
        &self,
        first: &[Idx],
        last: &[Idx],
        visit: &mut dyn FnMut(Element),
    ) -> Result<()> {
        let first = CellId::new(first.to_vec())?;
        let last = CellId::new(last.to_vec())?;
        let points = self.nbh.points();
        for (_, items) in self.nbh.grid().cells_in_range(&first, &last)? {
            for &i in items {
                let q = points.get(i).ok_or_else(|| {
                    Error::invalid(format!(
                        "neighbourhood {}: grid item {i} is not a point",
                        self.nbh.id()
                    ))
                })?;
                visit(Element::new(q.ty, q.element, q.ghost));
            }
        }
        Ok(())
    }
}

/// Upstream's dummy grid of a neighbourhood this rank does not have
/// (`non_local_manager.cc:216-231`: spacing 0, centre `Real::MAX`, no
/// point): its box (`+MAX` lower, `−MAX` upper corner) intersects no rank's
/// box, so the dummy grid synchroniser has empty schemes on every rank.
/// Unreachable from the solid model (module documentation).
#[derive(Clone, Debug)]
pub struct EmptyGridCells {
    lower: Vec<Real>,
    upper: Vec<Real>,
    spacing: Vec<Real>,
}

impl EmptyGridCells {
    /// The empty grid of dimension `dim`.
    #[must_use]
    pub fn new(dim: usize) -> Self {
        Self {
            lower: vec![Real::MAX; dim],
            upper: vec![-Real::MAX; dim],
            spacing: vec![0.; dim],
        }
    }
}

impl GridCells for EmptyGridCells {
    fn dimension(&self) -> usize {
        self.lower.len()
    }

    fn lower_bounds(&self) -> &[Real] {
        &self.lower
    }

    fn upper_bounds(&self) -> &[Real] {
        &self.upper
    }

    fn spacing(&self) -> &[Real] {
        &self.spacing
    }

    fn cell_index(&self, _position: Real, _direction: usize) -> Result<Idx> {
        Err(Error::invalid(
            "the dummy grid of a neighbourhood has no cell",
        ))
    }

    fn elements_in_cell_range(
        &self,
        _first: &[Idx],
        _last: &[Idx],
        _visit: &mut dyn FnMut(Element),
    ) -> Result<()> {
        Ok(())
    }
}

/// Upstream `createNeighborhoodSynchronizers`' name exchange
/// (`non_local_manager.cc:150-205`: `allReduce` of the longest name,
/// `allGather` of the counts, `allGatherV` of the zero-padded names): the
/// union of every rank's neighbourhood names, as a set (upstream
/// `std::set<ID> global_neighborhoods`). One phase of `ph`.
///
/// # Errors
/// A name holding a NUL byte (it could not be told apart from upstream's
/// padding), communication errors.
pub fn agree_neighborhood_names_in(
    ph: &mut Phases,
    local: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    let prepared = (|| {
        let mut bytes = Vec::new();
        for n in local {
            if n.as_bytes().contains(&0) {
                return Err(Error::invalid(format!(
                    "non-local neighbourhood name {n:?} holds a NUL byte"
                )));
            }
            bytes.extend_from_slice(n.as_bytes());
            bytes.push(0);
        }
        Ok(bytes)
    })();
    let all = ph.run_collective("non_local:neighborhood_names", prepared, |c, b| {
        c.all_gather_v(&b)
    })?;
    let mut names = BTreeSet::new();
    for block in all {
        for part in block.split(|&b| b == 0) {
            if part.is_empty() {
                continue;
            }
            let s = String::from_utf8(part.to_vec()).map_err(|_| {
                Error::Communication("non-local neighbourhood name is not UTF-8".into())
            })?;
            names.insert(s);
        }
    }
    Ok(names)
}

/// The grid synchronisers of a distributed non-local manager, one per
/// global neighbourhood name, and what the exchanges need to map elements
/// to the manager's slots.
#[derive(Debug, Default)]
pub struct GridGhosts {
    names: BTreeSet<String>,
    syncs: BTreeMap<String, Synchronizer<Element>>,
    layout: PointLayout,
    nb_quad: BTreeMap<ElementType, usize>,
}

impl GridGhosts {
    /// Empty machinery for the global names `names` (synchronisers added
    /// with [`GridGhosts::insert_synchronizer`]).
    #[must_use]
    pub fn new(names: BTreeSet<String>) -> Self {
        Self {
            names,
            ..Self::default()
        }
    }

    /// The global neighbourhood names (upstream `global_neighborhoods`).
    #[must_use]
    pub fn names(&self) -> &BTreeSet<String> {
        &self.names
    }

    /// Install the grid synchroniser of `name`.
    pub fn insert_synchronizer(&mut self, name: &str, sync: Synchronizer<Element>) {
        self.syncs.insert(name.to_owned(), sync);
    }

    /// The grid synchroniser of `name`.
    #[must_use]
    pub fn synchronizer(&self, name: &str) -> Option<&Synchronizer<Element>> {
        self.syncs.get(name)
    }

    /// Every grid synchroniser, by name.
    #[must_use]
    pub fn synchronizers(&self) -> &BTreeMap<String, Synchronizer<Element>> {
        &self.syncs
    }

    /// Every grid synchroniser, mutable (scheme updates after a removal).
    pub fn synchronizers_mut(&mut self) -> &mut BTreeMap<String, Synchronizer<Element>> {
        &mut self.syncs
    }

    /// Quadrature points per element of every regular type of the model's
    /// dimension (the FE engine's rule).
    pub fn set_nb_quad(&mut self, nb_quad: BTreeMap<ElementType, usize>) {
        self.nb_quad = nb_quad;
    }

    /// The point layout the exchanges map elements with.
    #[must_use]
    pub fn layout(&self) -> &PointLayout {
        &self.layout
    }

    /// The exchange object of one collective operation (its scope `ph`).
    pub fn exchange_in<'a>(&'a mut self, ph: &'a mut Phases) -> GridGhostExchange<'a> {
        GridGhostExchange { ghosts: self, ph }
    }
}

/// [`NonLocalGhostExchange`] over [`GridGhosts`] inside a collective scope.
pub struct GridGhostExchange<'a> {
    ghosts: &'a mut GridGhosts,
    ph: &'a mut Phases,
}

impl std::fmt::Debug for GridGhostExchange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridGhostExchange")
            .field("names", &self.ghosts.names)
            .finish_non_exhaustive()
    }
}

fn tag_of(tag: NonLocalTag) -> SynchronizationTag {
    match tag {
        NonLocalTag::ForAverage => SynchronizationTag::MnlForAverage,
        NonLocalTag::Weight => SynchronizationTag::MnlWeight,
    }
}

impl NonLocalGhostExchange for GridGhostExchange<'_> {
    fn is_distributed(&self) -> bool {
        true
    }

    fn global_names(&self) -> Option<&BTreeSet<String>> {
        Some(&self.ghosts.names)
    }

    fn on_layout(&mut self, layout: &PointLayout) -> Result<()> {
        self.ghosts.layout = layout.clone();
        Ok(())
    }

    fn wait(
        &mut self,
        neighborhood: &str,
        tag: NonLocalTag,
        fields: &mut [(&mut [Real], usize)],
    ) -> Result<()> {
        // a name of the agreed list without a synchroniser is a bookkeeping
        // error of this rank: reported at the scope's next agreement on
        // every rank (`akantu_comm::phase`)
        let sync = self.ghosts.syncs.get(neighborhood).ok_or_else(|| {
            Error::invalid(format!(
                "non-local manager: no grid synchroniser for the neighbourhood {neighborhood}"
            ))
        })?;
        let mut acc = SlotAccessor {
            layout: &self.ghosts.layout,
            nb_quad: &self.ghosts.nb_quad,
            fields,
        };
        sync.synchronize_in(self.ph, &mut acc, tag_of(tag))
    }
}

/// Element data at the manager's slots (upstream
/// `packElementalDataHelper` of each field in turn).
pub(crate) struct SlotAccessor<'b, 'f> {
    layout: &'b PointLayout,
    nb_quad: &'b BTreeMap<ElementType, usize>,
    fields: &'b mut [(&'f mut [Real], usize)],
}

impl<'b, 'f> SlotAccessor<'b, 'f> {
    /// An accessor of `fields` mapped with `layout` and `nb_quad`.
    pub(crate) fn new(
        layout: &'b PointLayout,
        nb_quad: &'b BTreeMap<ElementType, usize>,
        fields: &'b mut [(&'f mut [Real], usize)],
    ) -> Self {
        Self {
            layout,
            nb_quad,
            fields,
        }
    }

    fn nq(&self, ty: ElementType) -> Result<usize> {
        self.nb_quad.get(&ty).copied().ok_or_else(|| {
            Error::invalid(format!(
                "non-local exchange: no quadrature rule for {ty} (not a type of the model)"
            ))
        })
    }

    /// Slots of the points of `e` (`global_num = element·nq + q`).
    fn slots(&self, e: Element) -> Result<std::ops::Range<usize>> {
        let nq = self.nq(e.ty())?;
        let first = e
            .idx()
            .checked_mul(nq)
            .and_then(|v| Idx::try_from(v).ok())
            .ok_or_else(|| Error::invalid(format!("non-local exchange: {e} out of range")))?;
        let s0 = self
            .layout
            .slot(e.ty(), e.ghost_type(), first)
            .ok_or_else(|| {
                Error::invalid(format!(
                    "non-local exchange: {e} has no slot in the point layout"
                ))
            })?;
        if nq > 0 {
            let last = first + Idx::try_from(nq - 1).map_err(|_| Error::invalid("nq"))?;
            if self.layout.slot(e.ty(), e.ghost_type(), last) != Some(s0 + nq - 1) {
                return Err(Error::invalid(format!(
                    "non-local exchange: the points of {e} leave the point layout"
                )));
            }
        }
        Ok(s0..s0 + nq)
    }
}

impl DataAccessor<Element> for SlotAccessor<'_, '_> {
    fn nb_data(&self, elements: &[Element], _tag: SynchronizationTag) -> usize {
        let mut total = 0usize;
        for (_, n) in self.fields.iter() {
            for &e in elements {
                let Ok(nq) = self.nq(e.ty()) else {
                    return usize::MAX;
                };
                let Some(b) = nq.checked_mul(*n).and_then(|v| v.checked_mul(R)) else {
                    return usize::MAX;
                };
                let Some(t) = total.checked_add(b) else {
                    return usize::MAX;
                };
                total = t;
            }
        }
        total
    }

    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        _tag: SynchronizationTag,
    ) -> Result<()> {
        for (values, n) in self.fields.iter() {
            for &e in elements {
                for s in self.slots(e)? {
                    let v = values.get(s * n..(s + 1) * n).ok_or_else(|| {
                        Error::invalid("non-local exchange: field shorter than the layout")
                    })?;
                    for x in v {
                        buffer.pack(x);
                    }
                }
            }
        }
        Ok(())
    }

    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        _tag: SynchronizationTag,
    ) -> Result<()> {
        let slots: Vec<std::ops::Range<usize>> = elements
            .iter()
            .map(|&e| self.slots(e))
            .collect::<Result<_>>()?;
        for (values, n) in self.fields.iter_mut() {
            let n = *n;
            for r in &slots {
                for s in r.clone() {
                    let v = values.get_mut(s * n..(s + 1) * n).ok_or_else(|| {
                        Error::invalid("non-local exchange: field shorter than the layout")
                    })?;
                    for x in v.iter_mut() {
                        *x = buffer.unpack()?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Upstream `NonLocalNeighborhoodBase::getRelevantGhostElements` over every
/// neighbourhood (`NonLocalManager::cleanupExtraGhostElements`): the ghost
/// elements of a point in a pair (owned or ghost list).
#[must_use]
pub fn relevant_ghost_elements<'a>(
    neighborhoods: impl IntoIterator<Item = &'a NonLocalNeighborhood>,
) -> BTreeSet<Element> {
    let mut out = BTreeSet::new();
    for n in neighborhoods {
        for i in n.relevant_ghost_points() {
            let q = n.points()[i];
            out.insert(Element::new(q.ty, q.element, q.ghost));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::GhostType;

    #[test]
    fn empty_grid_box_is_inverted() {
        let g = EmptyGridCells::new(2);
        assert!(
            g.lower_bounds()
                .iter()
                .zip(g.upper_bounds())
                .all(|(l, u)| l > u)
        );
        assert!(g.cell_index(0., 0).is_err());
        let mut n = 0;
        g.elements_in_cell_range(&[0, 0], &[0, 0], &mut |_| n += 1)
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn slot_accessor_round_trip() {
        let layout = PointLayout::new([
            (ElementType::Triangle3, GhostType::NotGhost, 4),
            (ElementType::Triangle3, GhostType::Ghost, 2),
        ]);
        let nq: BTreeMap<ElementType, usize> = [(ElementType::Triangle3, 1)].into();
        let mut a: Vec<Real> = (0..12).map(Real::from).collect();
        let mut b: Vec<Real> = (100..106).map(Real::from).collect();
        let send = [Element::new(ElementType::Triangle3, 3, GhostType::NotGhost)];
        let recv = [Element::new(ElementType::Triangle3, 1, GhostType::Ghost)];
        let mut buf = CommunicationBuffer::new();
        {
            let mut fields: Vec<(&mut [Real], usize)> =
                vec![(a.as_mut_slice(), 2), (b.as_mut_slice(), 1)];
            let acc = SlotAccessor {
                layout: &layout,
                nb_quad: &nq,
                fields: &mut fields,
            };
            assert_eq!(acc.nb_data(&send, SynchronizationTag::MnlForAverage), 3 * R);
            acc.pack_data(&mut buf, &send, SynchronizationTag::MnlForAverage)
                .unwrap();
        }
        buf.rewind();
        {
            let mut fields: Vec<(&mut [Real], usize)> =
                vec![(a.as_mut_slice(), 2), (b.as_mut_slice(), 1)];
            let mut acc = SlotAccessor {
                layout: &layout,
                nb_quad: &nq,
                fields: &mut fields,
            };
            acc.unpack_data(&mut buf, &recv, SynchronizationTag::MnlForAverage)
                .unwrap();
        }
        // ghost slot 1 = layout slot 5: field a components (6, 7), field b 103
        assert_eq!(&a[10..12], &[6., 7.]);
        assert_eq!(b[5], 103.);
    }
}
