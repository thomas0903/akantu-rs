// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/cohesive_element_inserter_helper.{hh,cc}.

//! Facet and node doubling for cohesive insertion (upstream
//! `CohesiveElementInserterHelper`), on the journaled [`MeshTransaction`].
//!
//! Every edit of the mesh, the facet mesh and their adjacency data goes
//! through the transaction (rolled back on failure): node duplication on
//! [`MeshPart::Main`] (mirrored in the facet mesh), facet appends and
//! reconnections, and **row-journaled** edits of the facet mesh's
//! `element_to_subelement` / `subelement_to_element` (no snapshot of the
//! whole mesh data: a pass costs O(inserted facets × local neighbourhood),
//! a performance design gate of the port). No node→element map is used or
//! rebuilt (upstream v5.0.7 rebuilds one on every element event,
//! `mesh_inline_impl.hh:75`; the JOSS benchmark patched it out).
//!
//! The statement order of upstream is kept, so facet, node and cohesive
//! element numbering — hence DOF numbering — are upstream's. Facet events
//! are recorded on [`MeshPart::Facets`] at the points where upstream sends
//! them; the caller records the parent's element and node events.

use std::collections::{HashMap, VecDeque};

use akantu_core::{Element, ElementKind, ElementType, Error, GhostType, Idx, Result, TypeFilter};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT as E2S, SUBELEMENT_TO_ELEMENT as S2E};
use akantu_mesh::{Mesh, MeshEvent, MeshPart, MeshTransaction};

/// Origin of the facet events (upstream `AKANTU_CURRENT_FUNCTION` of the
/// helper's `sendEvent`s).
const ORIGIN: &str = "CohesiveElementInserterHelper";

/// The facet mesh of the transaction's mesh.
fn facets<'a>(tx: &'a MeshTransaction<'_>) -> Result<&'a Mesh> {
    tx.mesh()
        .mesh_facets()
        .ok_or_else(|| Error::Mesh("cohesive insertion needs the facet mesh".into()))
}

/// `element_to_subelement` row of `el` in the facet mesh.
fn e2s(fm: &Mesh, el: Element) -> Result<&Vec<Element>> {
    fm.data()
        .element_lists(E2S)?
        .element(el)?
        .first()
        .ok_or_else(|| Error::Mesh(format!("empty element_to_subelement row for {el}")))
}

/// `subelement_to_element` row of `el` in the facet mesh.
fn s2e(fm: &Mesh, el: Element) -> Result<&[Element]> {
    fm.data().element_map(S2E)?.element(el)
}

/// Upstream `hasElement(nodes_element, nodes)`: every node of `nodes` is in
/// `nodes_element`.
fn has_element(nodes_element: &[Idx], nodes: &[Idx]) -> bool {
    nodes.iter().all(|n| nodes_element.contains(n))
}

/// Upstream `removeElementsInVector(elem_to_remove, elem_list)`: when the
/// list is longer than the elements to remove, erase the first occurrence
/// of each; returns `(no deletion happened, new list)` — the new list is
/// `None` when unchanged.
fn remove_elements_in_vector(
    to_remove: &[Element],
    list: &[Element],
) -> Result<(bool, Option<Vec<Element>>)> {
    if list.len() <= to_remove.len() {
        return Ok((true, None));
    }
    let mut out = list.to_vec();
    let mut deletions = 0;
    for e in to_remove {
        if let Some(p) = out.iter().position(|x| x == e) {
            out.remove(p);
            deletions += 1;
        }
    }
    // upstream: AKANTU_DEBUG_ASSERT(deletions == 0 || deletions == size)
    if deletions != 0 && deletions != to_remove.len() {
        return Err(Error::Mesh(
            "cohesive insertion: not all elements around a subfacet were erased".into(),
        ));
    }
    Ok((deletions == 0, (deletions > 0).then_some(out)))
}

/// Facet / node doubling state of one insertion pass.
#[derive(Debug)]
pub(crate) struct InserterHelper {
    spatial_dimension: usize,
    /// Next index per `(type, ghost)` of the facet mesh and of the
    /// cohesive types (upstream `nb_new_facets`).
    nb_new: HashMap<(ElementType, GhostType), usize>,
    /// `(old, new)` pairs per element dimension (upstream
    /// `facets_to_double_by_dim`).
    to_double: [Vec<(Element, Element)>; 3],
    /// Upstream `dimelements_to_dimelements[dim1][dim2]`.
    dims: HashMap<(usize, usize), Vec<Vec<Element>>>,
    /// `(old node, new node)` (upstream `doubled_nodes`).
    pub(crate) doubled_nodes: Vec<(Idx, Idx)>,
    /// New cohesive elements (upstream `new_elements`).
    pub(crate) new_elements: Vec<Element>,
    /// Record every event of the insertion sequence even when empty (a
    /// distributed mesh: every rank records the same event kinds, ARCHITECTURE.md, "Staged mutation"; a rank without facet to double joins with empty lists).
    pub(crate) record_empty: bool,
}

impl InserterHelper {
    /// Upstream constructor: collect the facets flagged in `insertion`
    /// (ghost-major, type enum order, index order; or `order`, the global
    /// insertion order of a distributed mesh), skip boundary facets
    /// and facets next to a cohesive element, number the new facets and
    /// split the adjacency of each doubled facet (the second neighbour
    /// moves to the new facet; both facets become boundary facets).
    pub(crate) fn new(
        tx: &mut MeshTransaction<'_>,
        insertion: &akantu_core::ElementTypeMapArray<bool>,
        order: Option<&[Element]>,
    ) -> Result<Self> {
        let (spatial_dimension, mut nb_new, potential) = {
            let fm = facets(tx)?;
            let sd = fm.spatial_dimension();
            if order.is_none() && fm.is_distributed() && fm.communicator().size() > 1 {
                // upstream: global insertion order through the facet
                // synchronizer (`_ce_insertion_order`), computed before the
                // transaction by `CohesiveElementInserter::synchronize_insertion_in`
                return Err(Error::Unsupported(
                    "cohesive insertion on a mesh distributed over several ranks without the \
                     global insertion order (CohesiveElementInserter::synchronize_insertion_in)"
                        .into(),
                ));
            }
            let mut nb_new = HashMap::new();
            for ghost in GhostType::ALL {
                for ty in fm.element_types(TypeFilter {
                    spatial_dimension: None,
                    ghost_type: ghost,
                    kind: None,
                }) {
                    nb_new.insert((ty, ghost), fm.nb_element(ty, ghost));
                }
            }
            // the facets flagged for insertion: in the global insertion
            // order on a distributed mesh (upstream sorts them by
            // `global_orderings`), else ghost-major, type, index
            let potential = match order {
                Some(o) => o.to_vec(),
                None => super::parallel::flagged_facets(fm, insertion)?,
            };
            (sd, nb_new, potential)
        };
        let mut to_double: [Vec<(Element, Element)>; 3] = Default::default();
        for facet in potential {
            let row = e2s(facets(tx)?, facet)?.clone();
            let right = row.get(1).copied().unwrap_or(Element::NULL);
            if right.is_null() || right.kind() == ElementKind::Cohesive {
                // upstream: AKANTU_DEBUG_WARNING("attempt to double a facet
                // on the boundary") — compiled out in the oracle build
                continue;
            }
            let (ty, ghost) = (facet.ty(), facet.ghost_type());
            let counter = nb_new.entry((ty, ghost)).or_insert(0);
            let new_facet = Element::try_new(ty, *counter, ghost)?;
            *counter += 1;
            to_double[spatial_dimension - 1].push((facet, new_facet));
            // facet_to_element of the second neighbour → the new facet
            let mut s2e_row = s2e(facets(tx)?, right)?.to_vec();
            let pos = s2e_row
                .iter()
                .position(|&f| f == facet)
                .ok_or_else(|| Error::Mesh(format!("facet {facet} not found in {right}")))?;
            s2e_row[pos] = new_facet;
            tx.set_element_data_row(MeshPart::Facets, S2E, right, &s2e_row)?;
            // new facet: [old second neighbour, null]; old facet: [first, null]
            let mut new_row = row.clone();
            new_row[0] = new_row[1];
            new_row[1] = Element::NULL;
            let len = facets(tx)?.data().element_lists(E2S)?.get(ty, ghost)?.len();
            if len != new_facet.idx() {
                return Err(Error::Mesh(format!(
                    "element_to_subelement of {ty} has {len} rows, expected one per facet \
                     (adjacency contract, docs/ARCHITECTURE.md, mesh representation)"
                )));
            }
            tx.push_element_list_row(MeshPart::Facets, E2S, ty, ghost, new_row)?;
            let mut old_row = row;
            old_row[1] = Element::NULL;
            tx.set_element_list_row(MeshPart::Facets, E2S, facet, old_row)?;
        }
        Ok(Self {
            spatial_dimension,
            nb_new,
            to_double,
            dims: HashMap::new(),
            doubled_nodes: Vec::new(),
            new_elements: Vec::new(),
            record_empty: order.is_some(),
        })
    }

    /// Number of facets to double.
    pub(crate) fn nb_facets_to_double(&self) -> usize {
        self.to_double[self.spatial_dimension - 1].len()
    }

    /// The doubled facets `(old, new)`.
    pub(crate) fn doubled_facets(&self) -> &[(Element, Element)] {
        &self.to_double[self.spatial_dimension - 1]
    }

    fn dims_mut(&mut self, d1: usize, d2: usize) -> &mut Vec<Vec<Element>> {
        self.dims.entry((d1, d2)).or_default()
    }

    fn dims_of(&self, d1: usize, d2: usize) -> &[Vec<Element>] {
        self.dims.get(&(d1, d2)).map_or(&[], Vec::as_slice)
    }

    /// Upstream `insertCohesiveElement`: double facets and nodes, then
    /// create the cohesive elements. Returns the number of new elements.
    pub(crate) fn insert_cohesive_elements(
        &mut self,
        tx: &mut MeshTransaction<'_>,
    ) -> Result<usize> {
        let n = self.insert_facets_only(tx)?;
        if n == 0 && !self.record_empty {
            return Ok(0);
        }
        self.update_cohesive_data(tx)?;
        Ok(n)
    }

    /// Upstream `insertFacetsOnly`.
    pub(crate) fn insert_facets_only(&mut self, tx: &mut MeshTransaction<'_>) -> Result<usize> {
        let sd = self.spatial_dimension;
        if self.to_double[sd - 1].is_empty() && !self.record_empty {
            return Ok(0);
        }
        match sd {
            1 => self.double_point_facet(tx)?,
            2 => {
                self.double_facets(tx, 1)?;
                self.find_subfacet_to_double(tx, 1)?;
                self.double_subfacet(tx)?;
            }
            3 => {
                self.double_facets(tx, 2)?;
                self.find_subfacet_to_double(tx, 2)?;
                self.double_facets(tx, 1)?;
                self.find_subfacet_to_double(tx, 1)?;
                self.double_subfacet(tx)?;
            }
            _ => return Err(Error::Unsupported(format!("cohesive insertion in {sd}-D"))),
        }
        Ok(self.to_double[sd - 1].len())
    }

    /// Upstream `doubleNodes(old_nodes)`: append a copy of every node.
    fn double_nodes(&mut self, tx: &mut MeshTransaction<'_>, old_nodes: &[Idx]) -> Result<()> {
        for &old in old_nodes {
            let new = tx.duplicate_node(MeshPart::Main, old)?;
            self.doubled_nodes.push((old, new));
        }
        Ok(())
    }

    /// Upstream `updateElementalConnectivity(mesh, old, new, elements,
    /// facets)`: replace `old` by `new` in the elements of `part` (in a
    /// cohesive element only on the sides whose facet is in `facet_list`).
    fn update_elemental_connectivity(
        tx: &mut MeshTransaction<'_>,
        part: MeshPart,
        old_node: Idx,
        new_node: Idx,
        elements: &[Element],
        facet_list: Option<&[Element]>,
    ) -> Result<()> {
        for &el in elements {
            if el.is_null() {
                continue;
            }
            let mesh = match part {
                MeshPart::Main => tx.mesh(),
                MeshPart::Facets => facets(tx)?,
            };
            let mut conn = mesh.element_connectivity(el)?.to_vec();
            if el.kind() == ElementKind::Cohesive {
                let facet_list = facet_list.ok_or_else(|| {
                    Error::Mesh("provide a facet list to update cohesive elements".into())
                })?;
                let sides = s2e(facets(tx)?, el)?.to_vec();
                let nb = conn.len() / 2;
                for (n, side) in sides.iter().enumerate() {
                    if !facet_list.contains(side) {
                        continue;
                    }
                    let slot = conn[n * nb..(n + 1) * nb]
                        .iter_mut()
                        .find(|x| **x == old_node)
                        .ok_or_else(|| {
                            Error::Mesh(format!("node {old_node} not found in {el} (side {n})"))
                        })?;
                    *slot = new_node;
                }
            } else {
                let slot = conn
                    .iter_mut()
                    .find(|x| **x == old_node)
                    .ok_or_else(|| Error::Mesh(format!("node {old_node} not found in {el}")))?;
                *slot = new_node;
            }
            tx.set_element_nodes(part, el, &conn)?;
        }
        Ok(())
    }

    /// Upstream `updateSubelementToElement(dim, facet_mode)`.
    fn update_subelement_to_element(
        &self,
        tx: &mut MeshTransaction<'_>,
        dim: usize,
        facet_mode: bool,
    ) -> Result<()> {
        let lists = self.dims_of(dim + usize::from(facet_mode), dim);
        for ((old_sub, new_sub), list) in self.to_double[dim].iter().zip(lists) {
            for &facet in list {
                let mut row = s2e(facets(tx)?, facet)?.to_vec();
                let slot = row
                    .iter_mut()
                    .find(|x| **x == *old_sub)
                    .ok_or_else(|| Error::Mesh(format!("subfacet {old_sub} not found")))?;
                *slot = *new_sub;
                tx.set_element_data_row(MeshPart::Facets, S2E, facet, &row)?;
            }
        }
        Ok(())
    }

    /// Upstream `updateElementToSubelement(dim, facet_mode)`: one
    /// `element_to_subelement` row per element of dimension `dim` (new rows
    /// empty), then the rows of the new subfacets.
    fn update_element_to_subelement(
        &self,
        tx: &mut MeshTransaction<'_>,
        dim: usize,
        facet_mode: bool,
    ) -> Result<()> {
        let mut grow = Vec::new();
        {
            let fm = facets(tx)?;
            for ghost in GhostType::ALL {
                for ty in fm.element_types(TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: ghost,
                    kind: None,
                }) {
                    let n = fm.nb_element(ty, ghost);
                    let len = fm
                        .data()
                        .element_lists(E2S)?
                        .get(ty, ghost)
                        .map_or(0, akantu_core::Array::len);
                    if n > len {
                        grow.push((ty, ghost, n - len));
                    }
                }
            }
        }
        for (ty, ghost, k) in grow {
            for _ in 0..k {
                tx.push_element_list_row(MeshPart::Facets, E2S, ty, ghost, Vec::new())?;
            }
        }
        let lists = self.dims_of(dim + usize::from(facet_mode), dim);
        for ((_, new_sub), list) in self.to_double[dim].iter().zip(lists) {
            tx.set_element_list_row(MeshPart::Facets, E2S, *new_sub, list.clone())?;
        }
        Ok(())
    }

    /// Upstream `updateQuadraticSegments(dim)`: double the middle node of
    /// doubled `_segment_3` facets / edges.
    fn update_quadratic_segments(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        dim: usize,
    ) -> Result<()> {
        let sd = self.spatial_dimension;
        let list = self.to_double[dim].clone();
        let mut middle_nodes = Vec::new();
        for (old, _) in &list {
            if old.ty() != ElementType::Segment3 {
                continue;
            }
            let node = facets(tx)?.element_connectivity(*old)?[2];
            if !tx.mesh().is_pure_ghost_node(node)? {
                middle_nodes.push(node);
            }
        }
        let mut n = self.doubled_nodes.len();
        self.double_nodes(tx, &middle_nodes)?;
        for (k, (old, new)) in list.iter().enumerate() {
            if old.ty() != ElementType::Segment3 {
                continue;
            }
            let old_node = facets(tx)?.element_connectivity(*old)?[2];
            if tx.mesh().is_pure_ghost_node(old_node)? {
                continue;
            }
            let new_node = self.doubled_nodes[n].1;
            let mut conn = facets(tx)?.element_connectivity(*new)?.to_vec();
            conn[2] = new_node;
            tx.set_element_nodes(MeshPart::Facets, *new, &conn)?;
            let around = e2s(facets(tx)?, *new)?.clone();
            if dim + 2 == sd {
                Self::update_elemental_connectivity(
                    tx,
                    MeshPart::Facets,
                    old_node,
                    new_node,
                    &around,
                    None,
                )?;
                let elements = self
                    .dims_of(dim + 2, dim)
                    .get(k)
                    .cloned()
                    .unwrap_or_default();
                let facet_list = self
                    .dims_of(dim + 1, dim)
                    .get(k)
                    .cloned()
                    .unwrap_or_default();
                Self::update_elemental_connectivity(
                    tx,
                    MeshPart::Main,
                    old_node,
                    new_node,
                    &elements,
                    Some(&facet_list),
                )?;
            } else {
                Self::update_elemental_connectivity(
                    tx,
                    MeshPart::Main,
                    old_node,
                    new_node,
                    &around,
                    None,
                )?;
            }
            n += 1;
        }
        Ok(())
    }

    /// Upstream `doubleFacets<dim>()`: append the new facets of dimension
    /// `dim` (copies of the old connectivity and subfacet lists), register
    /// them with their subfacets, then the subfacet updates and the facet
    /// event.
    fn double_facets(&mut self, tx: &mut MeshTransaction<'_>, dim: usize) -> Result<()> {
        let sd = self.spatial_dimension;
        let list = self.to_double[dim].clone();
        let mut event = Vec::with_capacity(list.len());
        for &(old, new) in &list {
            let (conn, subs) = {
                let fm = facets(tx)?;
                (
                    fm.element_connectivity(old)?.to_vec(),
                    s2e(fm, old)?.to_vec(),
                )
            };
            let pushed = tx.push_element(MeshPart::Facets, old.ty(), old.ghost_type(), &conn)?;
            if pushed != new {
                return Err(Error::Mesh(format!(
                    "cohesive insertion: new facet numbered {pushed}, expected {new}"
                )));
            }
            tx.push_element_data_rows(
                MeshPart::Facets,
                S2E,
                old.ty(),
                old.ghost_type(),
                subs.len(),
                &subs,
            )?;
            for &sub in &subs {
                if sub.is_null() {
                    continue;
                }
                let mut row = e2s(facets(tx)?, sub)?.clone();
                row.push(new);
                tx.set_element_list_row(MeshPart::Facets, E2S, sub, row)?;
            }
            event.push(new);
        }
        if dim + 1 != sd {
            self.update_subelement_to_element(tx, dim, true)?;
            self.update_element_to_subelement(tx, dim, true)?;
        }
        self.update_quadratic_segments(tx, dim)?;
        tx.record(
            MeshPart::Facets,
            MeshEvent::ElementsAdded {
                elements: event,
                constitutive_law: None,
                origin: ORIGIN.into(),
                extension: None,
            },
        );
        Ok(())
    }

    /// Upstream `findElementsAroundSubfacet`: breadth-first walk from
    /// `start` through the facets containing `sub_conn`; returns whether
    /// `end_facet` was reached.
    fn find_elements_around_subfacet(
        fm: &Mesh,
        start: Element,
        end_facet: Element,
        sub_conn: &[Idx],
        element_list: &mut Vec<Element>,
        facet_list: &mut Vec<Element>,
        mut subfacet_list: Option<&mut Vec<Element>>,
    ) -> Result<bool> {
        let mut matched = false;
        element_list.push(start);
        let mut queue = VecDeque::from([start]);
        while let Some(&current) = queue.front() {
            let facets_of = s2e(fm, current)?;
            for &cf in facets_of {
                if cf.is_null() {
                    continue;
                }
                if cf == end_facet {
                    matched = true;
                }
                if facet_list.contains(&cf) || !has_element(&fm.element_connectivity(cf)?, sub_conn)
                {
                    continue;
                }
                facet_list.push(cf);
                if let Some(sl) = subfacet_list.as_deref_mut() {
                    for &cs in s2e(fm, cf)? {
                        if cs.is_null()
                            || sl.contains(&cs)
                            || !has_element(&fm.element_connectivity(cs)?, sub_conn)
                        {
                            continue;
                        }
                        sl.push(cs);
                    }
                }
                let around = e2s(fm, cf)?;
                let opposing = if around.first() == Some(&current) {
                    1
                } else {
                    0
                };
                let Some(&opp) = around.get(opposing) else {
                    continue;
                };
                if opp.is_null() || element_list.contains(&opp) {
                    continue;
                }
                if opp.kind() == ElementKind::Regular {
                    queue.push_back(opp);
                }
                element_list.push(opp);
            }
            queue.pop_front();
        }
        Ok(matched)
    }

    /// Upstream `findSubfacetToDouble<dim>()`.
    fn find_subfacet_to_double(&mut self, tx: &mut MeshTransaction<'_>, dim: usize) -> Result<()> {
        let sd = self.spatial_dimension;
        let list = self.to_double[sd - 1].clone();
        for f in 0..2 {
            for pair in &list {
                let (old_facet, new_facet) = if f == 0 { *pair } else { (pair.1, pair.0) };
                let (start, subs) = {
                    let fm = facets(tx)?;
                    (e2s(fm, new_facet)?[0], s2e(fm, old_facet)?.to_vec())
                };
                let current_facet = old_facet;
                for sub in subs {
                    if sub.is_null() {
                        continue;
                    }
                    if dim + 2 == sd {
                        let subsubs = s2e(facets(tx)?, sub)?.to_vec();
                        for subsub in subsubs {
                            if subsub.is_null() {
                                continue;
                            }
                            let mut element_list = Vec::new();
                            let mut facet_list = Vec::new();
                            let mut subfacet_list = Vec::new();
                            let (matched, removal) = {
                                let fm = facets(tx)?;
                                let conn = fm.element_connectivity(subsub)?.to_vec();
                                let matched = Self::find_elements_around_subfacet(
                                    fm,
                                    start,
                                    current_facet,
                                    &conn,
                                    &mut element_list,
                                    &mut facet_list,
                                    Some(&mut subfacet_list),
                                )?;
                                let removal = if matched {
                                    None
                                } else {
                                    Some(remove_elements_in_vector(
                                        &subfacet_list,
                                        e2s(fm, subsub)?,
                                    )?)
                                };
                                (matched, removal)
                            };
                            if matched {
                                continue;
                            }
                            let Some((none_removed, new_row)) = removal else {
                                continue;
                            };
                            if let Some(r) = new_row {
                                tx.set_element_list_row(MeshPart::Facets, E2S, subsub, r)?;
                            }
                            if none_removed {
                                continue;
                            }
                            let new_subsub = self.next_index(subsub)?;
                            self.to_double[dim - 1].push((subsub, new_subsub));
                            self.dims_mut(dim - 1, dim - 1).push(subfacet_list);
                            self.dims_mut(dim, dim - 1).push(facet_list);
                            self.dims_mut(dim + 1, dim - 1).push(element_list);
                        }
                    } else {
                        let mut element_list = Vec::new();
                        let mut facet_list = Vec::new();
                        let (matched, removal) = {
                            let fm = facets(tx)?;
                            let conn = fm.element_connectivity(sub)?.to_vec();
                            let matched = Self::find_elements_around_subfacet(
                                fm,
                                start,
                                current_facet,
                                &conn,
                                &mut element_list,
                                &mut facet_list,
                                None,
                            )?;
                            let removal = if matched {
                                None
                            } else {
                                Some(remove_elements_in_vector(&facet_list, e2s(fm, sub)?)?)
                            };
                            (matched, removal)
                        };
                        if matched {
                            continue;
                        }
                        let Some((none_removed, new_row)) = removal else {
                            continue;
                        };
                        if let Some(r) = new_row {
                            tx.set_element_list_row(MeshPart::Facets, E2S, sub, r)?;
                        }
                        if none_removed {
                            continue;
                        }
                        let new_sub = self.next_index(sub)?;
                        self.to_double[dim - 1].push((sub, new_sub));
                        self.dims_mut(dim, dim - 1).push(facet_list);
                        self.dims_mut(dim + 1, dim - 1).push(element_list);
                    }
                }
            }
        }
        Ok(())
    }

    /// `nb_new_facets(type, ghost)++` as an element.
    fn next_index(&mut self, of: Element) -> Result<Element> {
        let c = self.nb_new.entry((of.ty(), of.ghost_type())).or_insert(0);
        let e = Element::try_new(of.ty(), *c, of.ghost_type())?;
        *c += 1;
        Ok(e)
    }

    /// Upstream `doubleSubfacet<spatial_dimension>()` (2-D and 3-D): double
    /// the nodes of the doubled `_point_1` subfacets and reconnect.
    fn double_subfacet(&mut self, tx: &mut MeshTransaction<'_>) -> Result<()> {
        let sd = self.spatial_dimension;
        let list = self.to_double[0].clone();
        let mut nodes_to_double = Vec::with_capacity(list.len());
        for (old, _) in &list {
            if old.ty() != ElementType::Point1 {
                return Err(Error::Unsupported(
                    "only _point_1 subfacet doubling is supported (upstream assertion)".into(),
                ));
            }
            nodes_to_double.push(facets(tx)?.element_connectivity(*old)?[0]);
        }
        let first = self.doubled_nodes.len();
        self.double_nodes(tx, &nodes_to_double)?;
        let mut event = Vec::with_capacity(list.len());
        for (f, (_, new)) in list.iter().enumerate() {
            event.push(*new);
            let (old_node, new_node) = self.doubled_nodes[first + f];
            let pushed =
                tx.push_element(MeshPart::Facets, new.ty(), new.ghost_type(), &[new_node])?;
            if pushed != *new {
                return Err(Error::Mesh(format!(
                    "cohesive insertion: new subfacet numbered {pushed}, expected {new}"
                )));
            }
            let elements = self.dims_of(2, 0).get(f).cloned().unwrap_or_default();
            let facet_list = self.dims_of(1, 0).get(f).cloned().unwrap_or_default();
            Self::update_elemental_connectivity(
                tx,
                MeshPart::Main,
                old_node,
                new_node,
                &elements,
                Some(&facet_list),
            )?;
            Self::update_elemental_connectivity(
                tx,
                MeshPart::Facets,
                old_node,
                new_node,
                &facet_list,
                None,
            )?;
            if sd == 3 {
                let edges = self.dims_of(0, 0).get(f).cloned().unwrap_or_default();
                Self::update_elemental_connectivity(
                    tx,
                    MeshPart::Facets,
                    old_node,
                    new_node,
                    &edges,
                    None,
                )?;
            }
        }
        self.update_subelement_to_element(tx, 0, sd == 2)?;
        self.update_element_to_subelement(tx, 0, sd == 2)?;
        tx.record(
            MeshPart::Facets,
            MeshEvent::ElementsAdded {
                elements: event,
                constitutive_law: None,
                origin: ORIGIN.into(),
                extension: None,
            },
        );
        Ok(())
    }

    /// Upstream `doublePointFacet()` (1-D): double the node of every doubled
    /// point facet and reconnect the second neighbour.
    fn double_point_facet(&mut self, tx: &mut MeshTransaction<'_>) -> Result<()> {
        let list = self.to_double[0].clone();
        let mut event = Vec::with_capacity(list.len());
        for (old, new) in &list {
            let (element, old_node) = {
                let fm = facets(tx)?;
                (e2s(fm, *new)?[0], fm.element_connectivity(*old)?[0])
            };
            let new_node = tx.duplicate_node(MeshPart::Main, old_node)?;
            let pushed =
                tx.push_element(MeshPart::Facets, new.ty(), new.ghost_type(), &[new_node])?;
            if pushed != *new {
                return Err(Error::Mesh(format!(
                    "cohesive insertion: new point facet numbered {pushed}, expected {new}"
                )));
            }
            let mut conn = tx.mesh().element_connectivity(element)?.to_vec();
            let slot = conn
                .iter_mut()
                .find(|x| **x == old_node)
                .ok_or_else(|| Error::Mesh(format!("node {old_node} not found in {element}")))?;
            *slot = new_node;
            tx.set_element_nodes(MeshPart::Main, element, &conn)?;
            self.doubled_nodes.push((old_node, new_node));
            event.push(*new);
        }
        tx.record(
            MeshPart::Facets,
            MeshEvent::ElementsAdded {
                elements: event,
                constitutive_law: None,
                origin: ORIGIN.into(),
                extension: None,
            },
        );
        Ok(())
    }

    /// Upstream `updateCohesiveData()`: one cohesive element per doubled
    /// facet (connectivity: old facet then new facet, inverted in 3-D),
    /// recorded as the second neighbour of both facets, with its two facets
    /// in the facet mesh's `subelement_to_element`.
    fn update_cohesive_data(&mut self, tx: &mut MeshTransaction<'_>) -> Result<()> {
        let third = self.spatial_dimension == 3;
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = tx
                .mesh()
                .element_types(TypeFilter {
                    spatial_dimension: None,
                    ghost_type: ghost,
                    kind: Some(ElementKind::Cohesive),
                })
                .collect();
            for ty in types {
                let n = tx.mesh().nb_element(ty, ghost);
                self.nb_new.insert((ty, ghost), n);
            }
        }
        let list = self.to_double[self.spatial_dimension - 1].clone();
        self.new_elements.reserve(list.len());
        for (old, new) in list {
            let sides = if third { [new, old] } else { [old, new] };
            let ty_c = old
                .ty()
                .cohesive_type()
                .ok_or_else(|| Error::UnsupportedElementType {
                    ty: old.ty(),
                    context: "cohesive insertion (no cohesive type for this facet type)",
                })?;
            let ghost = old.ghost_type();
            let counter = self.nb_new.entry((ty_c, ghost)).or_insert(0);
            let cohesive = Element::try_new(ty_c, *counter, ghost)?;
            *counter += 1;
            let mut conn = Vec::with_capacity(ty_c.nb_nodes_per_element());
            for s in sides {
                conn.extend_from_slice(&facets(tx)?.element_connectivity(s)?);
            }
            for s in sides {
                let mut row = e2s(facets(tx)?, s)?.clone();
                if row.len() < 2 {
                    row.resize(2, Element::NULL);
                }
                row[1] = cohesive;
                tx.set_element_list_row(MeshPart::Facets, E2S, s, row)?;
            }
            let pushed = tx.push_element(MeshPart::Main, ty_c, ghost, &conn)?;
            if pushed != cohesive {
                return Err(Error::Mesh(format!(
                    "cohesive insertion: new cohesive element numbered {pushed}, expected \
                     {cohesive}"
                )));
            }
            let s2e_len = facets(tx)?
                .data()
                .element_map(S2E)
                .ok()
                .and_then(|m| m.get(ty_c, ghost).ok())
                .map_or(0, akantu_core::Array::len);
            if s2e_len != cohesive.idx() {
                return Err(Error::Mesh(format!(
                    "subelement_to_element of {ty_c} has {s2e_len} rows, expected one per \
                     cohesive element (adjacency contract, docs/ARCHITECTURE.md, mesh representation)"
                )));
            }
            tx.push_element_data_rows(MeshPart::Facets, S2E, ty_c, ghost, 2, &sides)?;
            self.new_elements.push(cohesive);
        }
        Ok(())
    }
}
