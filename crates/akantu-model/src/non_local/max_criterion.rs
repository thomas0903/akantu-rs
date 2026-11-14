// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_local_toolbox/
// neighborhoods_criterion_evaluation/neighborhood_max_criterion.{hh,cc},
// neighborhood_max_criterion_inline_impl.hh.

//! `NeighborhoodMaxCriterion` (NL-009): the integration points whose
//! criterion is maximal within the radius.
//!
//! Upstream this class is dead code: its data-accessor methods use stale
//! names, so it stays abstract and cannot be instantiated, and as written
//! it would never return a point (`findMaxQuads` zeroes `is_highest`
//! instead of setting it, and the self pair of every point marks the point
//! as not highest). The port implements the evident intent (UD-563,
//! category C): every owned point starts as "highest"; for each pair
//! `(q1, q2)` of the owned list in grid order (self pairs skipped),
//! `c(q1) < c(q2)` clears `q1`, otherwise `q2` is cleared; for ghost pairs
//! only `q1` can be cleared. Ties therefore go to the first point of the
//! pair, as upstream's comparison.
//!
//! **Distributed meshes** (MPI-050; no upstream behaviour to
//! reproduce: the class cannot be instantiated upstream, UD-563): the
//! intended `initNeighborhood` in stages driven by the model owning the
//! mesh ([`NeighborhoodMaxCriterion::new_owned`], then the grid
//! synchroniser — a mesh mutation —, [`NeighborhoodMaxCriterion::insert_ghost_quads`],
//! [`NeighborhoodMaxCriterion::update_pairs`], the removal of the ghost
//! elements the synchroniser added that no ghost pair holds,
//! [`NeighborhoodMaxCriterion::finish`]), and `findMaxQuads` with the
//! `_nh_criterion` exchange of the criterion at the ghost slots over the
//! grid synchroniser ([`NeighborhoodMaxCriterion::find_max_quads_in`];
//! upstream's pack / unpack methods carry stale names and never run). A
//! pair spanning two ranks is seen from both sides: each rank clears only
//! its own point, so the distributed answer equals the serial one whenever
//! no two neighbouring criteria are equal (a tie keeps both points there,
//! the serial first-point rule clears one).

use std::collections::{BTreeMap, BTreeSet};

use akantu_comm::Synchronizer;
use akantu_comm::phase::Phases;
use akantu_core::element_type_map::TypeFilter;
use akantu_core::enums::SynchronizationTag;
use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_parser::{Parsable, ParserSection};

use super::manager::PointLayout;
use super::neighborhood::NonLocalNeighborhood;
use super::point::{PointIdentity, QuadPoint};
use super::weight::BaseWeightFunction;

/// Upstream `NeighborhoodMaxCriterion` (serial).
#[derive(Debug)]
pub struct NeighborhoodMaxCriterion {
    criterion_id: String,
    neighborhood: NonLocalNeighborhood,
    layout: PointLayout,
    nb_quad: BTreeMap<ElementType, Idx>,
    /// The grid synchroniser of a distributed mesh (upstream
    /// `grid_synchronizer`, tag `_nh_criterion`).
    synchronizer: Option<Synchronizer<Element>>,
}

impl NeighborhoodMaxCriterion {
    /// Upstream constructor + `initNeighborhood`: parse `section` (a
    /// `neighborhood` section: `radius`), create the grid centred on the
    /// mesh bounding box, insert every integration point of every
    /// `(type, NotGhost)` block of the model's dimension (element order,
    /// point order), build the half pair lists. `criterion_id` names the
    /// internal read by [`NeighborhoodMaxCriterion::find_max_quads`].
    ///
    /// # Errors
    /// Parse errors, an empty bounding box, FE engine errors.
    pub fn new(
        mesh: &Mesh,
        fe: &FeEngine,
        criterion_id: &str,
        id: &str,
        section: &ParserSection,
        permissive: bool,
    ) -> Result<Self> {
        let mut c = Self::new_owned(mesh, fe, criterion_id, id, section, permissive)?;
        c.update_pairs()?;
        c.finish(mesh, fe)?;
        Ok(c)
    }

    /// The first stage of [`NeighborhoodMaxCriterion::new`]: section,
    /// grid and the owned integration points (no pair, no layout yet).
    ///
    /// # Errors
    /// Parse errors, an empty bounding box, FE engine errors.
    pub fn new_owned(
        mesh: &Mesh,
        fe: &FeEngine,
        criterion_id: &str,
        id: &str,
        section: &ParserSection,
        permissive: bool,
    ) -> Result<Self> {
        let dim = mesh.spatial_dimension();
        let mut nh =
            NonLocalNeighborhood::new(id, id, dim, Box::new(BaseWeightFunction::default()));
        let mut s = section.clone();
        s.section_type = akantu_parser::ParserType::NonLocal;
        nh.parse_section(&s, permissive)?;
        let (lo, hi) = (mesh.lower_bounds(), mesh.upper_bounds());
        if lo
            .iter()
            .zip(hi)
            .any(|(l, h)| l.partial_cmp(h).is_none_or(std::cmp::Ordering::is_gt))
        {
            return Err(Error::invalid(
                "neighborhood max criterion: the mesh bounding box is empty",
            ));
        }
        let center: Vec<Real> = lo.iter().zip(hi).map(|(l, h)| (h + l) / 2.).collect();
        nh.init_neighborhood(&center)?;
        let mut c = Self {
            criterion_id: criterion_id.to_owned(),
            neighborhood: nh,
            layout: PointLayout::default(),
            nb_quad: BTreeMap::new(),
            synchronizer: None,
        };
        c.insert_quads(mesh, fe, GhostType::NotGhost)?;
        Ok(c)
    }

    /// Upstream `insertAllQuads(ghost)`: every integration point of every
    /// element of the regular `(type, ghost)` blocks of the mesh's
    /// dimension (element order, point order).
    fn insert_quads(&mut self, mesh: &Mesh, fe: &FeEngine, ghost: GhostType) -> Result<()> {
        let dim = mesh.spatial_dimension();
        let filter = TypeFilter::new()
            .dim(dim)
            .ghost(ghost)
            .kind(ElementKind::Regular);
        for ty in mesh.element_types(filter) {
            let nq = fe.nb_integration_points(ty, ghost)?;
            let ne = mesh.nb_element(ty, ghost);
            let mut xs = akantu_core::Array::new(dim);
            fe.integration_points_coordinates(mesh, &mut xs, ty, ghost, None)?;
            let nq_i =
                Idx::try_from(nq).map_err(|_| Error::invalid("too many quadrature points"))?;
            self.nb_quad.insert(ty, nq_i);
            for e in 0..ne {
                let e_i = Idx::try_from(e).map_err(|_| Error::invalid("too many elements"))?;
                for q in 0..nq {
                    let q_i = Idx::try_from(q).map_err(|_| Error::invalid("too many points"))?;
                    let p = QuadPoint::new(ty, ghost, e_i, q_i, nq_i);
                    self.neighborhood
                        .insert_integration_point(p, xs.tuple(e * nq + q))?;
                }
            }
        }
        Ok(())
    }

    /// The ghost stage (distributed meshes): every ghost point of the mesh
    /// as it is after the grid synchroniser (upstream `insertAllQuads(_ghost)`).
    ///
    /// # Errors
    /// FE engine or grid errors.
    pub fn insert_ghost_quads(&mut self, mesh: &Mesh, fe: &FeEngine) -> Result<()> {
        self.insert_quads(mesh, fe, GhostType::Ghost)
    }

    /// Upstream `updatePairList()`.
    ///
    /// # Errors
    /// Grid errors.
    pub fn update_pairs(&mut self) -> Result<()> {
        self.neighborhood.update_pair_list(
            PointIdentity::UpstreamGlobalNum,
            akantu_core::math::DEFAULT_TOLERANCE,
        )
    }

    /// The ghost elements holding the second point of a ghost pair
    /// (upstream `cleanupExtraGhostElements`' set).
    #[must_use]
    pub fn relevant_ghost_elements(&self) -> BTreeSet<Element> {
        let nh = &self.neighborhood;
        nh.pair_list(GhostType::Ghost)
            .iter()
            .map(|&(_, j)| {
                let q = nh.points()[j];
                Element::new(q.ty, q.element, q.ghost)
            })
            .collect()
    }

    /// The neighbourhood after a removal of ghost elements (see
    /// [`super::NonLocalManager::renumber_ghost_points`]).
    ///
    /// # Errors
    /// A removed element holding a point of a pair.
    pub fn renumber_ghost_points(
        &mut self,
        new_numbering: &ElementTypeMapArray<Idx>,
        nb_quad: &dyn Fn(ElementType) -> Result<usize>,
    ) -> Result<()> {
        self.neighborhood
            .renumber_ghost_points(new_numbering, nb_quad)
    }

    /// The grid view of the neighbourhood (the grid synchroniser's input).
    #[must_use]
    pub fn grid_cells(&self) -> super::ghosts::NeighborhoodGridCells<'_> {
        super::ghosts::NeighborhoodGridCells::new(&self.neighborhood)
    }

    /// Install the grid synchroniser of a distributed mesh.
    pub fn set_synchronizer(&mut self, sync: Synchronizer<Element>) {
        self.synchronizer = Some(sync);
    }

    /// The grid synchroniser of a distributed mesh.
    #[must_use]
    pub fn synchronizer(&self) -> Option<&Synchronizer<Element>> {
        self.synchronizer.as_ref()
    }

    /// The grid synchroniser, mutable (scheme updates after a removal).
    pub fn synchronizer_mut(&mut self) -> Option<&mut Synchronizer<Element>> {
        self.synchronizer.as_mut()
    }

    /// The last stage: the per-point layout of every regular block of both
    /// ghost types and the slot of every point.
    ///
    /// # Errors
    /// FE engine errors; a point outside the layout.
    pub fn finish(&mut self, mesh: &Mesh, fe: &FeEngine) -> Result<()> {
        let dim = mesh.spatial_dimension();
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            let filter = TypeFilter::new()
                .dim(dim)
                .ghost(g)
                .kind(ElementKind::Regular);
            for ty in mesh.element_types(filter) {
                let nq = fe.nb_integration_points(ty, g)?;
                blocks.push((ty, g, mesh.nb_element(ty, g) * nq));
                let nq_i =
                    Idx::try_from(nq).map_err(|_| Error::invalid("too many quadrature points"))?;
                self.nb_quad.insert(ty, nq_i);
            }
        }
        // serial: owned blocks only (as before the distributed support)
        if self.synchronizer.is_none() {
            blocks.retain(|b| b.1 == GhostType::NotGhost);
        }
        self.layout = PointLayout::new(blocks);
        let mut slots = Vec::new();
        for q in self.neighborhood.points() {
            slots.push(
                self.layout
                    .slot(q.ty, q.ghost, q.global_num)
                    .ok_or_else(|| {
                        Error::invalid("neighborhood max criterion: point outside the layout")
                    })?,
            );
        }
        self.neighborhood.slots = slots;
        Ok(())
    }

    /// Name of the criterion internal.
    #[must_use]
    pub fn criterion_id(&self) -> &str {
        &self.criterion_id
    }

    /// The per-point layout (`criterion` of
    /// [`NeighborhoodMaxCriterion::find_max_quads`] is indexed by its slots).
    #[must_use]
    pub fn layout(&self) -> &PointLayout {
        &self.layout
    }

    /// The underlying neighbourhood (grid, pairs).
    #[must_use]
    pub fn neighborhood(&self) -> &NonLocalNeighborhood {
        &self.neighborhood
    }

    /// Upstream `findMaxQuads` (evident intent, module docs): the owned
    /// points whose `criterion` (one value per layout slot, upstream the
    /// flattened internal `criterion_id`) is maximal in their neighbourhood,
    /// in block order then point order.
    ///
    /// # Errors
    /// `criterion` shorter than the layout.
    pub fn find_max_quads(&self, criterion: &[Real]) -> Result<Vec<QuadPoint>> {
        self.max_quads(criterion)
    }

    /// [`NeighborhoodMaxCriterion::find_max_quads`] on a distributed mesh,
    /// one phase of `ph`: the intended `_nh_criterion` exchange first (the
    /// owners' criterion written at the ghost slots of `criterion` over the
    /// grid synchroniser), then the comparison. Without a synchroniser
    /// (serial) only the comparison.
    ///
    /// # Errors
    /// `criterion` shorter than the layout, exchange errors.
    pub fn find_max_quads_in(
        &self,
        ph: &mut Phases,
        criterion: &mut [Real],
    ) -> Result<Vec<QuadPoint>> {
        if let Some(sync) = &self.synchronizer {
            let nb_quad: BTreeMap<ElementType, usize> = self
                .nb_quad
                .iter()
                .map(|(&t, &n)| (t, usize::try_from(n).unwrap_or(0)))
                .collect();
            let mut fields: Vec<(&mut [Real], usize)> = vec![(criterion, 1)];
            let mut acc = super::ghosts::SlotAccessor::new(&self.layout, &nb_quad, &mut fields);
            sync.synchronize_in(ph, &mut acc, SynchronizationTag::NhCriterion)?;
        }
        self.max_quads(criterion)
    }

    fn max_quads(&self, criterion: &[Real]) -> Result<Vec<QuadPoint>> {
        if criterion.len() < self.layout.len() {
            return Err(Error::DimensionMismatch {
                context: "NeighborhoodMaxCriterion::find_max_quads",
                expected: self.layout.len(),
                got: criterion.len(),
            });
        }
        let nh = &self.neighborhood;
        let mut highest = vec![true; self.layout.len()];
        for g in GhostType::ALL {
            for &(i, j) in nh.pair_list(g) {
                if i == j {
                    continue;
                }
                let (s1, s2) = (nh.slots[i], nh.slots[j]);
                if criterion[s1] < criterion[s2] {
                    highest[s1] = false;
                } else if g != GhostType::Ghost {
                    highest[s2] = false;
                }
            }
        }
        let mut out = Vec::new();
        for (ty, g, off, n) in self.layout.blocks() {
            if g == GhostType::Ghost || n == 0 {
                continue;
            }
            let nq = self.nb_quad.get(&ty).copied().unwrap_or(1).max(1);
            for k in 0..n {
                if highest[off + k] {
                    let k_i = Idx::try_from(k).map_err(|_| Error::invalid("too many points"))?;
                    out.push(QuadPoint::new(ty, g, k_i / nq, k_i % nq, nq));
                }
            }
        }
        Ok(out)
    }
}
