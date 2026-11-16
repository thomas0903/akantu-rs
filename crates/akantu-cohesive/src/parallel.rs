// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/solid_mechanics_model_cohesive_parallel.cc
// (updateCohesiveSynchronizers, updateFacetStressSynchronizer,
// getNbData / packData / unpackData of `_smmc_facets_stress`,
// `_constitutive_law_id` and the cohesive `_smm_stress`),
// solid_mechanics_model_cohesive.cc (constructor: split of the cohesive
// synchronizer; initFullImpl: facet stress synchronizer;
// CohesiveMeshGlobalDataUpdater::updateData),
// materials/material_cohesive_inline_impl.hh (getNbData / packData /
// unpackData of `_smm_stress` and `_smmc_damage`),
// src/synchronizer/element_synchronizer.cc (buildElementToPrank, getRank).

//! The cohesive model on a distributed mesh (COH-008, PART-011,
//! MPI-037…039).
//!
//! # Synchronizers
//!
//! * **cohesive synchronizer** (upstream `cohesive_synchronizer`): the
//!   cohesive elements split off the mesh's element synchronizer at model
//!   construction ([`akantu_comm::Synchronizer::split_from`]: one scheme per
//!   neighbour and direction, empty unless the mesh already held cohesive
//!   elements) and extended **without communication** on every insertion:
//!   for each facet of each facet-synchronizer scheme (same rank and
//!   direction), in scheme order, the new cohesive element in the facet's
//!   `element_to_subelement` row (slot 1) is appended
//!   ([`extend_cohesive_synchronizer`]). Both sides walk the same facet
//!   order, so the scheme pairs stay matched.
//! * **facet-stress synchronizer** (upstream `facet_stress_synchronizer`,
//!   extrinsic): a copy of the facet synchronizer with send and receive
//!   swapped (the ghost side sends the stress of its own element to the
//!   facet's owner), filtered locally to the checked facets that lie between
//!   this rank and the scheme's rank ([`filter_facet_stress_synchronizer`],
//!   at `init_automatic_insertion`, `update_automatic_insertion` and after
//!   every insertion).
//! * owner ranks of the bulk elements (upstream `element_to_prank` of the
//!   mesh's element synchronizer), computed once: bulk elements never
//!   change owner.
//!
//! # Insertion (the distributed hook, upstream `CohesiveMeshGlobalDataUpdater`)
//!
//! [`CohesiveGlobalDataUpdater`] runs in stage 2 of the insertion mutation
//! (ARCHITECTURE), on a copy of the synchronizers installed only when
//! every rank committed: the cohesive synchronizer is extended, the
//! facet-stress synchronizer filtered (inserted facets are no longer
//! checked), the doubled nodes get global ids and owners
//! ([`akantu_mesh::global_ids_updater::update_global_ids_in`] over the
//! extended cohesive synchronizer) and the global node count, the node
//! synchronizer is rebuilt
//! ([`akantu_mesh::synchronizers::rebuild_node_schemes_in`]), and the owners
//! send the staged law of every new cohesive element to its ghost copies
//! (`_constitutive_law_id`): the dispatch appends the ghosts to their laws
//! in receive order ([`akantu_solid::model::SolidMechanicsPhysics::staged_ghost_laws`]).
//! The doubled nodes already carry the flags of the nodes they double
//! (`MeshTransaction::duplicate_node`), as upstream's `updateData` sets them.
//!
//! # Exchanges
//!
//! `_smm_stress` of cohesive elements ([`TractionAccessor`]: per law in law
//! order, the tractions then the contact tractions of the law's elements at
//! every quadrature point) after the not-ghost tractions are computed, so
//! the ghost cohesive elements are assembled with their owner's tractions;
//! `_smmc_facets_stress` ([`FacetStressAccessor`]) after the stress
//! interpolation; `_smmc_damage` ([`DamageAccessor`]) on request.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::Phases;
use akantu_comm::synchronizer::SendRecv;
use akantu_comm::{DataAccessor, Synchronizer};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Rank, Real,
    Result,
};
use akantu_mesh::utils::ELEMENT_TO_SUBELEMENT;
use akantu_mesh::{Mesh, MeshEvent, MeshPart, MeshTransaction};
use akantu_model::ConstitutiveLawsHandler;
use akantu_solid::Material;

use crate::material::{MaterialCohesive, block, block_mut};

/// Bytes of one `Real`.
const R: usize = std::mem::size_of::<Real>();

/// The law decisions staged for the new cohesive elements of an insertion
/// (shared with the model's staged selector).
pub(crate) type StagedLaws = Arc<Mutex<HashMap<Element, usize>>>;

/// Laws of new ghost cohesive elements in unpack order, shared by the hook
/// (writer) and the model's event handler (reader).
pub type GhostLaws = Arc<Mutex<Vec<(Element, usize)>>>;

/// Synchronizers of the cohesive model on a distributed mesh (see the
/// module documentation).
#[derive(Clone, Debug)]
pub struct CohesiveSynchronizers {
    /// Upstream `cohesive_synchronizer`.
    pub cohesive: Synchronizer<Element>,
    /// Upstream `facet_stress_synchronizer` (extrinsic models, after
    /// `init_full`).
    pub facet_stress: Option<Synchronizer<Element>>,
    /// Owner rank of every element of the mesh dimension at construction
    /// (upstream `element_to_prank`; bulk elements). Shared: never changed
    /// after construction, so the copy an insertion stages does not copy it
    /// (performance).
    pub element_to_prank: Arc<ElementTypeMapArray<Rank>>,
}

impl CohesiveSynchronizers {
    /// Upstream constructor part: the cohesive synchronizer split off the
    /// mesh's element synchronizer `element` (cohesive elements move,
    /// `element` keeps the others) and the owner ranks.
    ///
    /// # Errors
    /// Mesh errors.
    pub fn split_from(mesh: &Mesh, element: &mut Synchronizer<Element>, id: &str) -> Result<Self> {
        let mut cohesive = Synchronizer::new(
            format!("{id}:cohesive_distributed_synchronizer"),
            element.communicator.clone(),
        );
        cohesive.split_from(element, |e| e.kind() == ElementKind::Cohesive);
        let element_to_prank = Arc::new(akantu_mesh::distribution::element_to_prank(mesh)?);
        Ok(Self {
            cohesive,
            facet_stress: None,
            element_to_prank,
        })
    }
}

/// Upstream `initFullImpl`: the facet-stress synchronizer, a copy of the
/// facet synchronizer `facets` with send and receive swapped.
#[must_use]
pub fn facet_stress_synchronizer(
    facets: &Synchronizer<Element>,
    id: &str,
) -> Synchronizer<Element> {
    let mut s = facets.clone();
    s.id = format!("{id}:facet_stress_synchronizer");
    s.swap_send_recv();
    s
}

/// Upstream `updateFacetStressSynchronizer` (and the second part of
/// `updateCohesiveSynchronizers`): keep, in every scheme toward `proc`, the
/// facets with `check(facet)` whose two neighbours are owned by this rank
/// and by `proc` (in either order). Local: both sides of a scheme pair
/// decide the same facets.
///
/// # Errors
/// Malformed facet adjacency or check map.
pub fn filter_facet_stress_synchronizer(
    syncs: &mut CohesiveSynchronizers,
    mesh_facets: &Mesh,
    check: &dyn Fn(Element) -> Result<bool>,
) -> Result<()> {
    let rank = mesh_facets.communicator().rank();
    let e2s = mesh_facets.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
    let Some(fss) = syncs.facet_stress.as_mut() else {
        return Ok(());
    };
    let mut failure: Option<Error> = None;
    let prank = &syncs.element_to_prank;
    let rank_of = |e: Element| -> Rank {
        if e.is_null() {
            return -1;
        }
        prank
            .element(e)
            .ok()
            .and_then(|v| v.first().copied())
            .unwrap_or(-1)
    };
    fss.update_schemes(|scheme, proc, _| {
        scheme.retain(|&f| {
            if failure.is_some() {
                return true;
            }
            let keep = (|| -> Result<bool> {
                if !check(f)? {
                    return Ok(false);
                }
                let row = &e2s.element(f)?[0];
                let left = row.first().copied().unwrap_or(Element::NULL);
                let right = row.get(1).copied().unwrap_or(Element::NULL);
                let (rl, rr) = (rank_of(left), rank_of(right));
                Ok((rl == proc && rr == rank) || (rl == rank && rr == proc))
            })();
            keep.unwrap_or_else(|e| {
                failure = Some(e);
                true
            })
        });
    });
    failure.map_or(Ok(()), Err)
}

/// Upstream `updateCohesiveSynchronizers`, first part: for every scheme
/// `(proc, direction)` of the cohesive synchronizer, the cohesive element in
/// slot 1 of the `element_to_subelement` row of each facet of the facet
/// synchronizer's scheme `(proc, direction)` is appended when it is new
/// (index at least the count before the insertion, `old_counts`). Returns
/// the appended elements per `(proc, direction)`, in order.
///
/// # Errors
/// A scheme of the cohesive synchronizer without its facet scheme, malformed
/// adjacency.
pub fn extend_cohesive_synchronizer(
    cohesive: &mut Synchronizer<Element>,
    facets: &Synchronizer<Element>,
    mesh_facets: &Mesh,
    old_counts: &BTreeMap<(ElementType, GhostType), usize>,
) -> Result<Vec<(Rank, SendRecv, Vec<Element>)>> {
    let e2s = mesh_facets.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
    let mut appended = Vec::new();
    let mut failure: Option<Error> = None;
    cohesive.update_schemes(|scheme, proc, sr| {
        if failure.is_some() {
            return;
        }
        let r = (|| -> Result<Vec<Element>> {
            let Some(facet_scheme) = facets.communications.schemes(sr).get(&proc) else {
                return Err(Error::Communication(format!(
                    "cohesive synchronizer: no facet scheme {sr:?} toward rank {proc} \
                     (upstream getScheme throws)"
                )));
            };
            let mut new = Vec::new();
            for &facet in facet_scheme {
                let row = &e2s.element(facet)?[0];
                let Some(&c) = row.get(1) else {
                    continue;
                };
                if c.is_null() || c.kind() != ElementKind::Cohesive {
                    continue;
                }
                let ct = facet
                    .ty()
                    .cohesive_type()
                    .ok_or(Error::UnsupportedElementType {
                        ty: facet.ty(),
                        context: "updateCohesiveSynchronizers (no cohesive type)",
                    })?;
                let old = old_counts
                    .get(&(ct, facet.ghost_type()))
                    .copied()
                    .unwrap_or(0);
                if c.idx() >= old {
                    new.push(c);
                }
            }
            Ok(new)
        })();
        match r {
            Ok(new) => {
                scheme.extend_from_slice(&new);
                appended.push((proc, sr, new));
            }
            Err(e) => failure = Some(e),
        }
    });
    failure.map_or(Ok(appended), Err)
}

/// Elements of `elements` split by law (upstream `splitByConstitutiveLaw`):
/// per law in order, `(element, law-local index)` in input order.
fn split_by_law(
    materials: &ConstitutiveLawsHandler<dyn Material>,
    elements: &[Element],
) -> Result<Vec<Vec<(Element, usize)>>> {
    let mut out: Vec<Vec<(Element, usize)>> = vec![Vec::new(); materials.nb_laws()];
    for &e in elements {
        let li = materials.law_index_map().element_ref(e)?[0];
        let law = akantu_core::decode_idx(li)?
            .ok_or_else(|| Error::invalid(format!("{e} has no constitutive law")))?;
        let local = akantu_core::try_to_usize(materials.local_numbering_map().element(e)?[0])?;
        out.get_mut(law)
            .ok_or_else(|| Error::invalid(format!("{e}: law {law} out of range")))?
            .push((e, local));
    }
    Ok(out)
}

fn cohesive_of(law: &dyn Material) -> Result<&MaterialCohesive> {
    law.as_any()
        .and_then(|a| a.downcast_ref::<MaterialCohesive>())
        .ok_or_else(|| Error::invalid(format!("law '{}' is not cohesive", law.name())))
}

fn cohesive_of_mut(law: &mut dyn Material) -> Result<&mut MaterialCohesive> {
    let name = law.name().to_owned();
    law.as_any_mut()
        .and_then(|a| a.downcast_mut::<MaterialCohesive>())
        .ok_or_else(|| Error::invalid(format!("law '{name}' is not cohesive")))
}

/// Which internals of a cohesive law an accessor exchanges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fields {
    /// `_smm_stress`: tractions, then contact tractions (`dim` each).
    Tractions,
    /// `_smmc_damage`: damage (1 per point).
    Damage,
}

/// Cohesive-element accessor of `_smm_stress` ([`TractionAccessor`]) and
/// `_smmc_damage` ([`DamageAccessor`]): upstream `MaterialCohesive::packData`
/// through `splitByConstitutiveLaw` (per law in order; per field, every
/// element's values at its quadrature points, element-major).
pub struct CohesiveFieldAccessor<'a> {
    materials: &'a mut ConstitutiveLawsHandler<dyn Material>,
    /// Quadrature points per cohesive type (`CohesiveFEEngine`).
    nb_quad: &'a BTreeMap<ElementType, usize>,
    dim: usize,
    fields: Fields,
}

impl std::fmt::Debug for CohesiveFieldAccessor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CohesiveFieldAccessor")
            .field("fields", &self.fields)
            .field("dim", &self.dim)
            .finish_non_exhaustive()
    }
}

/// `_smm_stress` of cohesive elements.
pub type TractionAccessor<'a> = CohesiveFieldAccessor<'a>;
/// `_smmc_damage` of cohesive elements.
pub type DamageAccessor<'a> = CohesiveFieldAccessor<'a>;

impl<'a> CohesiveFieldAccessor<'a> {
    /// `_smm_stress` accessor.
    pub fn tractions(
        materials: &'a mut ConstitutiveLawsHandler<dyn Material>,
        nb_quad: &'a BTreeMap<ElementType, usize>,
        dim: usize,
    ) -> Self {
        Self {
            materials,
            nb_quad,
            dim,
            fields: Fields::Tractions,
        }
    }

    /// `_smmc_damage` accessor.
    pub fn damage(
        materials: &'a mut ConstitutiveLawsHandler<dyn Material>,
        nb_quad: &'a BTreeMap<ElementType, usize>,
        dim: usize,
    ) -> Self {
        Self {
            materials,
            nb_quad,
            dim,
            fields: Fields::Damage,
        }
    }

    fn tag(&self) -> SynchronizationTag {
        match self.fields {
            Fields::Tractions => SynchronizationTag::SmmStress,
            Fields::Damage => SynchronizationTag::SmmcDamage,
        }
    }

    fn nq(&self, ty: ElementType) -> Result<usize> {
        self.nb_quad
            .get(&ty)
            .copied()
            .ok_or_else(|| Error::not_found("integration points of", ty.name()))
    }

    /// Per point components of each exchanged field.
    fn components(&self) -> Vec<usize> {
        match self.fields {
            Fields::Tractions => vec![self.dim, self.dim],
            Fields::Damage => vec![1],
        }
    }
}

impl DataAccessor<Element> for CohesiveFieldAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag != self.tag() {
            return 0;
        }
        let per: usize = self.components().iter().sum();
        let mut size = 0usize;
        for e in elements {
            let Ok(nq) = self.nq(e.ty()) else {
                return usize::MAX;
            };
            size = size.saturating_add(per.saturating_mul(nq).saturating_mul(R));
        }
        size
    }

    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != self.tag() {
            return Ok(());
        }
        let split = split_by_law(self.materials, elements)?;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let c = cohesive_of(self.materials.law(law)?)?;
            let fields: Vec<(&akantu_model::internal_field::InternalField<Real>, usize)> =
                match self.fields {
                    Fields::Tractions => vec![
                        (&c.core.tractions, self.dim),
                        (&c.core.contact_tractions, self.dim),
                    ],
                    Fields::Damage => vec![(&c.core.damage, 1)],
                };
            for (field, nbc) in fields {
                // one guard per (type, ghost) block, re-taken on a change
                let mut current: Option<(
                    (ElementType, GhostType),
                    akantu_core::SliceRef<'_, Real>,
                )> = None;
                for &(e, local) in els {
                    let key = (e.ty(), e.ghost_type());
                    if current.as_ref().is_none_or(|(k, _)| *k != key) {
                        current = Some((key, block(field, e.ty(), e.ghost_type())));
                    }
                    let nq = self.nq(e.ty())?;
                    let vals = current.as_ref().map(|(_, s)| &s[..]).unwrap_or(&[]);
                    let start = local * nq * nbc;
                    let v = vals.get(start..start + nq * nbc).ok_or_else(|| {
                        Error::invalid(format!(
                            "{e}: cohesive internal has no values (law-local {local})"
                        ))
                    })?;
                    buffer.pack_slice(v);
                }
            }
        }
        Ok(())
    }

    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != self.tag() {
            return Ok(());
        }
        let split = split_by_law(self.materials, elements)?;
        let dim = self.dim;
        let mode = self.fields;
        for (law, els) in split.iter().enumerate() {
            if els.is_empty() {
                continue;
            }
            let nqs: Vec<usize> = els
                .iter()
                .map(|(e, _)| self.nq(e.ty()))
                .collect::<Result<_>>()?;
            let c = cohesive_of_mut(self.materials.law_mut(law)?)?;
            let nb_fields = if mode == Fields::Tractions { 2 } else { 1 };
            for k in 0..nb_fields {
                let (field, nbc) = match (mode, k) {
                    (Fields::Tractions, 0) => (&mut c.core.tractions, dim),
                    (Fields::Tractions, _) => (&mut c.core.contact_tractions, dim),
                    (Fields::Damage, _) => (&mut c.core.damage, 1),
                };
                let mut i = 0;
                while i < els.len() {
                    // a run of elements of one (type, ghost) block: one guard
                    let key = (els[i].0.ty(), els[i].0.ghost_type());
                    let mut a = block_mut(field, key.0, key.1)?;
                    while i < els.len() && (els[i].0.ty(), els[i].0.ghost_type()) == key {
                        let (e, local) = els[i];
                        let nq = nqs[i];
                        let start = local * nq * nbc;
                        let v = a
                            .as_mut_slice()
                            .get_mut(start..start + nq * nbc)
                            .ok_or_else(|| {
                                Error::invalid(format!(
                                    "{e}: cohesive internal has no values (law-local {local})"
                                ))
                            })?;
                        buffer.unpack_slice(v)?;
                        i += 1;
                    }
                }
            }
        }
        Ok(())
    }
}

/// `_smmc_facets_stress` accessor (upstream
/// `packUnpackFacetStressDataHelper`): per facet, at every facet quadrature
/// point, the `dim²` stress of one side — packed: side 1 if the facet's
/// first neighbour is a ghost, else side 0 (the ghost side sends the stress
/// of its own element); unpacked: side 1 if the first neighbour is local,
/// else side 0.
pub struct FacetStressAccessor<'a> {
    /// The model's `facet_stress` (`2·dim²` per facet quadrature point).
    pub facet_stress: &'a mut ElementTypeMapArray<Real>,
    /// The facet mesh.
    pub mesh_facets: &'a Mesh,
    /// Spatial dimension.
    pub dim: usize,
}

impl std::fmt::Debug for FacetStressAccessor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FacetStressAccessor")
            .field("dim", &self.dim)
            .finish_non_exhaustive()
    }
}

impl FacetStressAccessor<'_> {
    fn side(&self, f: Element, pack: bool) -> Result<usize> {
        let e2s = self
            .mesh_facets
            .data()
            .element_lists(ELEMENT_TO_SUBELEMENT)?;
        let first = e2s.element(f)?[0].first().copied().unwrap_or(Element::NULL);
        let not_ghost = first.ghost_type() == GhostType::NotGhost;
        Ok(if pack {
            usize::from(!not_ghost)
        } else {
            usize::from(not_ghost)
        })
    }
}

impl DataAccessor<Element> for FacetStressAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag != SynchronizationTag::SmmcFacetsStress {
            return 0;
        }
        let dd = self.dim * self.dim;
        let mut size = 0usize;
        for e in elements {
            let Ok(nq) = crate::material::nb_quad_facet(e.ty()) else {
                return usize::MAX;
            };
            size = size.saturating_add(nq.saturating_mul(dd).saturating_mul(R));
        }
        size
    }

    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::SmmcFacetsStress {
            return Ok(());
        }
        let dd = self.dim * self.dim;
        for &f in elements {
            if f.is_null() {
                return Err(Error::invalid(
                    "packUnpackFacetStressDataHelper called with wrong inputs",
                ));
            }
            let side = self.side(f, true)?;
            let nq = crate::material::nb_quad_facet(f.ty())?;
            let a = self.facet_stress.get(f.ty(), f.ghost_type())?.as_slice();
            for q in 0..nq {
                let start = ((f.idx() * nq + q) * 2 + side) * dd;
                let v = a
                    .get(start..start + dd)
                    .ok_or_else(|| Error::invalid(format!("facet stress of {f} not allocated")))?;
                buffer.pack_slice(v);
            }
        }
        Ok(())
    }

    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::SmmcFacetsStress {
            return Ok(());
        }
        let dd = self.dim * self.dim;
        for &f in elements {
            if f.is_null() {
                return Err(Error::invalid(
                    "packUnpackFacetStressDataHelper called with wrong inputs",
                ));
            }
            let side = self.side(f, false)?;
            let nq = crate::material::nb_quad_facet(f.ty())?;
            let a = self
                .facet_stress
                .get_mut(f.ty(), f.ghost_type())?
                .as_mut_slice();
            for q in 0..nq {
                let start = ((f.idx() * nq + q) * 2 + side) * dd;
                let v = a
                    .get_mut(start..start + dd)
                    .ok_or_else(|| Error::invalid(format!("facet stress of {f} not allocated")))?;
                buffer.unpack_slice(v)?;
            }
        }
        Ok(())
    }
}

/// `_constitutive_law_id` of new cohesive elements (upstream
/// `ConstitutiveLawsHandler` pack/unpack over the cohesive synchronizer): the
/// owners send the staged law of each element; a ghost element records the
/// received law in `ghost_laws` (unpack order) and in the staged map.
struct NewLawIdAccessor<'a> {
    staged: &'a StagedLaws,
    ghost_laws: &'a mut Vec<(Element, usize)>,
}

impl DataAccessor<Element> for NewLawIdAccessor<'_> {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag == SynchronizationTag::ConstitutiveLawId {
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
        if tag != SynchronizationTag::ConstitutiveLawId {
            return Ok(());
        }
        let staged = self
            .staged
            .lock()
            .map_err(|_| Error::invalid("staged cohesive law selection poisoned"))?;
        for e in elements {
            let law = staged.get(e).copied().ok_or_else(|| {
                Error::invalid(format!("new cohesive element {e} has no staged law"))
            })?;
            buffer.pack(&akantu_core::try_idx(law)?);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::ConstitutiveLawId {
            return Ok(());
        }
        let mut staged = self
            .staged
            .lock()
            .map_err(|_| Error::invalid("staged cohesive law selection poisoned"))?;
        for &e in elements {
            let law: Idx = buffer.unpack()?;
            let law = akantu_core::decode_idx(law)?.ok_or_else(|| {
                Error::Communication(format!("{e}: received no constitutive law from its owner"))
            })?;
            self.ghost_laws.push((e, law));
            staged.insert(e, law);
        }
        Ok(())
    }
}

/// Upstream `CohesiveMeshGlobalDataUpdater` (see the module documentation):
/// the distributed hook of the insertion mutation.
pub struct CohesiveGlobalDataUpdater<'a> {
    /// The synchronizers, staged (installed by the model after commit).
    pub staged: CohesiveSynchronizers,
    /// Check flag of every facet after the insertion (upstream
    /// `updateInsertionFacets` runs before `updateData`: inserted facets are
    /// no longer checked).
    pub check_after: &'a dyn Fn(Element) -> Result<bool>,
    /// Staged laws of the new not-ghost cohesive elements (input) and of the
    /// new ghost ones (output).
    pub staged_laws: StagedLaws,
    /// Laws of the new ghost cohesive elements in unpack order (output).
    pub ghost_laws: GhostLaws,
    /// The model has laws (the law-id exchange runs only then, on every
    /// rank alike).
    pub exchange_laws: bool,
    /// Every rank committed (set by [`akantu_mesh::DistributedHook::commit`]).
    pub committed: bool,
    /// New cohesive elements over every rank, ghost copies included
    /// (upstream `updateData`'s `allReduce(nb_new_elements)`; the value
    /// `checkCohesiveStress` returns).
    pub global_new_elements: u64,
    new_elements: Vec<Element>,
    nb_new_nodes: usize,
}

impl std::fmt::Debug for CohesiveGlobalDataUpdater<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CohesiveGlobalDataUpdater")
            .field("staged", &self.staged)
            .field("committed", &self.committed)
            .field("global_new_elements", &self.global_new_elements)
            .finish_non_exhaustive()
    }
}

impl<'a> CohesiveGlobalDataUpdater<'a> {
    /// A hook over a copy of `syncs`.
    #[must_use]
    pub fn new(
        syncs: &CohesiveSynchronizers,
        check_after: &'a dyn Fn(Element) -> Result<bool>,
        staged_laws: StagedLaws,
        exchange_laws: bool,
    ) -> Self {
        Self {
            staged: syncs.clone(),
            check_after,
            staged_laws,
            ghost_laws: Arc::default(),
            exchange_laws,
            committed: false,
            global_new_elements: 0,
            new_elements: Vec::new(),
            nb_new_nodes: 0,
        }
    }
}

impl akantu_mesh::DistributedHook for CohesiveGlobalDataUpdater<'_> {
    fn prepare(&mut self, _mesh: &Mesh, events: &[(MeshPart, MeshEvent)]) -> Result<()> {
        self.new_elements.clear();
        self.nb_new_nodes = 0;
        for (part, e) in events {
            if *part != MeshPart::Main {
                continue;
            }
            match e {
                MeshEvent::ElementsAdded { elements, .. } => {
                    self.new_elements.extend(
                        elements
                            .iter()
                            .copied()
                            .filter(|el| el.kind() == ElementKind::Cohesive),
                    );
                }
                MeshEvent::NodesAdded {
                    nodes,
                    old_nodes: Some(_),
                    ..
                } => self.nb_new_nodes += nodes.len(),
                MeshEvent::NodesAdded { .. } => {
                    return Err(Error::invalid(
                        "cohesive insertion hook: a node addition that doubles no node",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn apply(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        _events: &[(MeshPart, MeshEvent)],
        ph: &mut Phases,
    ) -> Result<()> {
        // ---- cohesive synchronizer and facet-stress synchronizer (local) --
        let appended = {
            let m = tx.mesh();
            let fm = m
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("cohesive insertion without facet mesh".into()))?;
            let facet_sync = fm.synchronizers().element.as_ref().ok_or_else(|| {
                Error::Mesh("distributed facet mesh without facet synchronizer".into())
            })?;
            let mut old: BTreeMap<(ElementType, GhostType), usize> = BTreeMap::new();
            for ghost in GhostType::ALL {
                for ty in m.element_types(akantu_core::TypeFilter {
                    spatial_dimension: None,
                    ghost_type: ghost,
                    kind: Some(ElementKind::Cohesive),
                }) {
                    old.insert((ty, ghost), m.nb_element(ty, ghost));
                }
            }
            for e in &self.new_elements {
                if let Some(c) = old.get_mut(&(e.ty(), e.ghost_type())) {
                    *c = c.saturating_sub(1);
                }
            }
            let appended =
                extend_cohesive_synchronizer(&mut self.staged.cohesive, facet_sync, fm, &old)?;
            filter_facet_stress_synchronizer(&mut self.staged, fm, self.check_after)?;
            appended
        };
        // ---- global ids of the doubled nodes (collective) -----------------
        let up = akantu_mesh::global_ids_updater::update_global_ids_in(
            ph,
            tx.mesh(),
            Some(&self.staged.cohesive),
            self.nb_new_nodes,
        )?;
        let n = tx.mesh().nb_nodes();
        if up.global_ids.len() != n {
            return Err(Error::DimensionMismatch {
                context: "cohesive insertion: global ids of the doubled nodes",
                expected: n,
                got: up.global_ids.len(),
            });
        }
        for node in n - self.nb_new_nodes.min(n)..n {
            let id = up.global_ids[node];
            let node = akantu_core::try_idx(node)?;
            if tx.mesh().node_global_id(node) != id {
                tx.set_node_global_id(MeshPart::Main, node, id)?;
            }
        }
        for (&node, &r) in &up.pranks {
            if tx.mesh().node_prank(node) != r {
                tx.set_node_prank(MeshPart::Main, node, Some(r))?;
            }
        }
        if tx.mesh().nb_global_nodes() != up.nb_global_nodes {
            tx.set_nb_global_nodes(MeshPart::Main, up.nb_global_nodes)?;
        }
        // upstream `comm.allReduce(nb_new_elements, _sum)` (the event list:
        // not-ghost and ghost new elements)
        let local = self.new_elements.len() as u64;
        self.global_new_elements =
            ph.run_collective("cohesive:nb_new_elements", Ok(local), |c, n| {
                let mut t = [n];
                c.all_reduce(&mut t, akantu_comm::ReduceOp::Sum)?;
                Ok(t[0])
            })?;
        // ---- upstream's Mesh::sendEvent<NewNodesEvent> -------------------
        // only when nodes were added: upstream sends the
        // CohesiveNewNodesEvent only when nb_new_nodes = updateGlobalIDs(…)
        // > 0 (the global count of new local / master nodes, the same on
        // every rank; solid_mechanics_model_cohesive.cc:76-91). Its
        // Mesh::sendEvent (mesh.cc:113-150) first rewrites the dimension − 1
        // connectivities from the facet mesh and rebuilds the element
        // groups' node lists (ElementGroup::onNodesAdded), then the handlers
        // run, NodeSynchronizer::onNodesAdded among them. Rebuilding the
        // groups at every insertion dropped pure-ghost group members too
        // early (UD-1250); the node schemes have the
        // same gate.
        if up.total_new > 0 {
            crate::inserter::sync_boundary_connectivity(tx)?;
            tx.rebuild_element_group_nodes(MeshPart::Main)?;
            if let Some(mut ns) = tx.mesh().synchronizers().node.clone() {
                akantu_mesh::synchronizers::rebuild_node_schemes_in(ph, tx.mesh(), &mut ns)?;
                // journaled replacement (no copy of the other synchronizers)
                tx.replace_node_synchronizer(MeshPart::Main, Some(ns))?;
            }
        }
        // ---- laws of the new ghost cohesive elements (owner -> ghost) -----
        if self.exchange_laws {
            let mut tmp = Synchronizer::new(
                format!("{}:new", self.staged.cohesive.id),
                self.staged.cohesive.communicator.clone(),
            );
            for (proc, sr, new) in appended {
                *tmp.communications.scheme_mut(proc, sr) = new;
            }
            let mut ghost_laws = Vec::new();
            {
                let mut acc = NewLawIdAccessor {
                    staged: &self.staged_laws,
                    ghost_laws: &mut ghost_laws,
                };
                tmp.synchronize_in(ph, &mut acc, SynchronizationTag::ConstitutiveLawId)?;
            }
            *self
                .ghost_laws
                .lock()
                .map_err(|_| Error::invalid("staged ghost laws poisoned"))? = ghost_laws;
        }
        Ok(())
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}
