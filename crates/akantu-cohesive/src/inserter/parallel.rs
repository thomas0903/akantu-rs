// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/cohesive_element_inserter.cc
// (insertElements: `_ce_groups` synchronisation),
// src/mesh_utils/cohesive_element_inserter_inline_impl.hh (getNbData /
// packData / unpackData of `_ce_groups`),
// src/mesh_utils/cohesive_element_inserter_helper.cc (constructor: global
// insertion order through `_ce_insertion_order`),
// src/mesh_utils/cohesive_element_inserter_parallel.cc (empty upstream).

//! Insertion decision on a distributed mesh (PART-011).
//!
//! Before the facets are doubled, every rank must know the same inserted
//! facets in the same order (the order fixes the numbering of the doubled
//! nodes, facets and cohesive elements, hence the global ids):
//!
//! 1. **`_ce_groups`** — the insertion flag of every facet travels from its
//!    owner to the ranks where it is a ghost, over the facet synchronizer
//!    (owner → ghost: a ghost facet takes its owner's flag, set or not);
//! 2. **global order** — every rank counts its flagged *not-ghost* facets;
//!    an exclusive scan gives its first number; the rank numbers all its
//!    flagged facets consecutively from there (not-ghost first, then ghost;
//!    facet types in enum order, facet order); the numbers of ghost facets
//!    are then replaced by their owner's (**`_ce_insertion_order`**, owner →
//!    ghost over the facet synchronizer);
//! 3. the flagged facets are sorted by that number (upstream `std::sort`;
//!    the numbers are unique — a duplicate, which upstream would order in an
//!    unspecified way, is refused, UD-815).
//!
//! The steps run in one [`Phases`] scope of the caller, before the mesh
//! transaction (they change no mesh state; the flags they overwrite are
//! restored by the model when the insertion fails before commit). On a mesh
//! that is not distributed the order is the serial one and nothing is
//! communicated.

use akantu_comm::DataAccessor;
use akantu_comm::ReduceOp;
use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::Phases;
use akantu_core::enums::SynchronizationTag;
use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Result,
    TypeFilter,
};
use akantu_mesh::Mesh;

use super::CohesiveElementInserter;

/// The facets flagged in `insertion` (ghost-major, facet types in enum
/// order, facet order): upstream's `potential_facets_to_double` before the
/// sort, and the serial insertion order.
pub(crate) fn flagged_facets(
    fm: &Mesh,
    insertion: &ElementTypeMapArray<bool>,
) -> Result<Vec<Element>> {
    let sd = fm.spatial_dimension();
    let mut out = Vec::new();
    for ghost in GhostType::ALL {
        for ty in fm.element_types(TypeFilter {
            spatial_dimension: Some(sd - 1),
            ghost_type: ghost,
            kind: Some(ElementKind::Regular),
        }) {
            let Ok(flags) = insertion.get(ty, ghost) else {
                continue;
            };
            for (i, &f) in flags.as_slice().iter().enumerate() {
                if f {
                    out.push(Element::try_new(ty, i, ghost)?);
                }
            }
        }
    }
    Ok(out)
}

fn not_ghost_count(facets: &[Element]) -> u64 {
    facets
        .iter()
        .filter(|e| e.ghost_type() == GhostType::NotGhost)
        .count() as u64
}

/// `_ce_groups` accessor (upstream `CohesiveElementInserter` pack/unpack):
/// one flag per facet.
struct GroupsAccessor<'a> {
    flags: &'a mut ElementTypeMapArray<bool>,
}

impl DataAccessor<Element> for GroupsAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag == SynchronizationTag::CeGroups {
            elements.len()
        } else {
            0
        }
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::CeGroups {
            return Ok(());
        }
        for &e in elements {
            buffer.pack(&self.flags.element(e)?[0]);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::CeGroups {
            return Ok(());
        }
        for &e in elements {
            let v: bool = buffer.unpack()?;
            self.flags.element_mut(e)?[0] = v;
        }
        Ok(())
    }
}

/// `_ce_insertion_order` accessor (upstream `SimpleElementDataAccessor<Int>`
/// over `global_orderings`): one number per facet.
struct OrderAccessor<'a> {
    order: &'a mut ElementTypeMapArray<Idx>,
}

impl DataAccessor<Element> for OrderAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag == SynchronizationTag::CeInsertionOrder {
            elements.len().saturating_mul(std::mem::size_of::<Idx>())
        } else {
            0
        }
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::CeInsertionOrder {
            return Ok(());
        }
        for &e in elements {
            buffer.pack(&self.order.element(e)?[0]);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::CeInsertionOrder {
            return Ok(());
        }
        for &e in elements {
            let v: Idx = buffer.unpack()?;
            self.order.element_mut(e)?[0] = v;
        }
        Ok(())
    }
}

/// Result of [`CohesiveElementInserter::synchronize_insertion_in`].
#[derive(Clone, Debug, Default)]
pub struct InsertionOrder {
    /// The facets to double on this rank (not-ghost and ghost), in the
    /// global insertion order.
    pub facets: Vec<Element>,
    /// Flagged not-ghost facets over every rank (0: nothing to insert
    /// anywhere; every rank sees the same value).
    pub global_flagged: u64,
}

impl CohesiveElementInserter {
    /// Steps 1–3 of the module documentation in `ph` (collective on a
    /// distributed mesh: every rank calls it, also with nothing flagged).
    /// The ghost facets' insertion flags are overwritten by their owners'.
    ///
    /// # Errors
    /// A mesh without facet mesh or facet synchronizer (distributed), an
    /// exchange failure (agreed in `ph`), duplicate insertion numbers
    /// (UD-815, agreed).
    pub fn synchronize_insertion_in(
        &mut self,
        ph: &mut Phases,
        mesh: &Mesh,
    ) -> Result<InsertionOrder> {
        let fm = mesh
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("the cohesive inserter needs the facet mesh".into()))?;
        if !fm.is_distributed() {
            let facets = flagged_facets(fm, &self.insertion_facets)?;
            let n = not_ghost_count(&facets);
            return Ok(InsertionOrder {
                facets,
                global_flagged: n,
            });
        }
        let sync = fm.synchronizers().element.as_ref().ok_or_else(|| {
            Error::Mesh("distributed facet mesh without facet synchronizer".into())
        })?;
        // 1. owner -> ghost insertion flags
        let mut groups = GroupsAccessor {
            flags: &mut self.insertion_facets,
        };
        sync.synchronize_in(ph, &mut groups, SynchronizationTag::CeGroups)?;
        // 2. numbers: exclusive scan of the flagged not-ghost facets
        let (counted, potential) = match flagged_facets(fm, &self.insertion_facets) {
            Ok(p) => (Ok(not_ghost_count(&p)), p),
            Err(e) => (Err(e), Vec::new()),
        };
        let (start, total) = ph.run_collective("ce:insertion_order", counted, |c, n| {
            let start = c.exclusive_scan_sum_u64(n)?;
            let mut t = [n];
            c.all_reduce(&mut t, ReduceOp::Sum)?;
            Ok((start, t[0]))
        })?;
        let sd = fm.spatial_dimension();
        let mut order = ElementTypeMapArray::<Idx>::new("global_orderings");
        for ghost in GhostType::ALL {
            for ty in fm.element_types(TypeFilter {
                spatial_dimension: Some(sd - 1),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            }) {
                order.alloc(fm.nb_element(ty, ghost), 1, ty, ghost, -1)?;
            }
        }
        let first = Idx::try_from(start).map_err(|_| Error::invalid("insertion order"))?;
        for (next, &f) in (first..).zip(&potential) {
            order.element_mut(f)?[0] = next;
        }
        let mut acc = OrderAccessor { order: &mut order };
        sync.synchronize_in(ph, &mut acc, SynchronizationTag::CeInsertionOrder)?;
        // 3. sort by the global numbers (unique: UD-815)
        let keyed: Result<Vec<(Idx, Element)>> = potential
            .iter()
            .map(|&f| Ok((order.element(f)?[0], f)))
            .collect();
        let checked = keyed.and_then(|mut k| {
            k.sort_by_key(|&(o, _)| o);
            if let Some(w) = k.windows(2).find(|w| w[0].0 == w[1].0) {
                return Err(Error::invalid(format!(
                    "cohesive insertion: the facets {} and {} share the insertion number {}: \
                     a flagged ghost facet that is in no receive scheme of the facet \
                     synchronizer kept a local number (upstream sorts them in an unspecified \
                     order); refused (UPSTREAM_DEFECTS UD-815)",
                    w[0].1, w[1].1, w[0].0
                )));
            }
            Ok(k.into_iter().map(|(_, f)| f).collect::<Vec<Element>>())
        });
        let facets = ph.agreed(
            "ce:insertion_order:unique",
            checked,
            akantu_comm::phase::Severity::Recoverable,
        )?;
        Ok(InsertionOrder {
            facets,
            global_flagged: total,
        })
    }
}

/// Facet types of `fm` (dimension `sd − 1`, regular kind) of both ghost
/// types, as `(type, ghost)` pairs.
#[allow(dead_code)] // used by the facet-stress synchronizer update
pub(crate) fn facet_blocks(fm: &Mesh) -> Vec<(ElementType, GhostType)> {
    let sd = fm.spatial_dimension();
    let mut out = Vec::new();
    for ghost in GhostType::ALL {
        for ty in fm.element_types(TypeFilter {
            spatial_dimension: Some(sd - 1),
            ghost_type: ghost,
            kind: Some(ElementKind::Regular),
        }) {
            out.push((ty, ghost));
        }
    }
    out
}
