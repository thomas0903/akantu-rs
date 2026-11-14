// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/facet_synchronizer.hh,
// facet_synchronizer.cc, facet_synchronizer_inline_impl.hh; src/mesh/mesh.cc
// (FacetGlobalConnectivityAccessor, distributed part of Mesh::initMeshFacets).

//! Facet synchronizer (upstream `FacetSynchronizer`, MPI-020) and the
//! distributed facet mesh (upstream `Mesh::initMeshFacets` on a distributed
//! mesh, PART-012).
//!
//! # Facet ownership and schemes
//!
//! On a distributed mesh the facets of dimension `d − 1` built by
//! `buildAllFacets` carry a ghost type: a facet between a local and a ghost
//! element is local on the lower of the two owner ranks and ghost on the
//! other ([`crate::utils`] `plan_facets`, upstream "ugly trick"), a facet of
//! ghost elements only is ghost. The facet synchronizer is an element
//! synchronizer of the facet mesh over those facets:
//!
//! * **owner of a ghost facet** = the **minimum rank** among the owners of
//!   the ghost elements (element receive schemes) it bounds;
//! * **receive scheme from `p`** (one per element receive scheme, created
//!   even when empty): for every ghost element received from `p`, in scheme
//!   order, its ghost facets (`subelement_to_element` order) owned by `p`,
//!   each once;
//! * **send scheme toward `q`** (one per rank that announced facets, even
//!   when empty): the local facets whose **sorted global connectivity**
//!   matches the ones `q` announced, in `q`'s receive-scheme order (upstream
//!   matches with `std::find` over the local facets: the first local facet
//!   with that sorted global connectivity).
//!
//! Upstream announces the connectivities with one synchronous message per
//! (facet type of the sender's local facets, receiving rank) and receives
//! them with `receiveAnyNumber`, matched against the receiver's own local
//! facet types, so its send schemes are type-major while the receive
//! schemes follow the element order. Here one message per receiving rank
//! lists the ghost facets **in receive-scheme order** (`[type code, sorted
//! connectivity…]` per facet) and is received with the same NBX protocol
//! ([`Communicator::receive_any_number`]); messages are processed in rank
//! order (each one only appends to its sender's scheme, so the arrival
//! order is irrelevant). Where every element has facets of one type (all
//! upstream tests) the schemes are identical to upstream's; with elements
//! carrying several facet types (prisms, pyramids) upstream deadlocks when
//! neighbouring ranks have different local facet types and pairs its send
//! and receive schemes in different orders otherwise (UD-251), while the
//! port's schemes match entry by entry.
//!
//! # Distributed facet mesh
//!
//! [`create_distributed_mesh_facets_in`] runs upstream's `initMeshFacets`
//! steps inside one [`Phases`] scope: the facets are built locally (agreed),
//! the facet synchronizer is built (one NBX phase), the global facet
//! connectivities are sent from the owners to the ghosts
//! (`_smmc_facets_conn`), the ghost facets are reordered to their owner's
//! node order ([`crate::utils::flip_facets_local_with`]; corrected default
//! [`FacetFlipMode::RemoteOrder`], `UpstreamInverse` reproduces UD-122) and
//! the parent's `physical_names` are transferred (agreed). Nothing is
//! installed in the parent mesh: the set-up operation
//! [`crate::utils::init_mesh_facets`] installs the result after every rank
//! succeeded; a mesh transaction installs a prepared facet mesh with
//! [`crate::MeshTransaction::install_mesh_facets`].
//!
//! A mesh of spatial dimension 1 on more than one rank is refused (UD-1141,
//! category C): upstream's `flipFacets` throws on its ghost `_point_1`
//! facets, so upstream never builds a distributed 1-D facet mesh (and no
//! distributed 1-D cohesive model).
//!
//! # Updates on mesh events (cohesive insertion)
//!
//! Upstream rebuilds nothing here on insertion: the cohesive model
//! re-synchronizes the global facet connectivities of new ghost facets
//! (`SolidMechanicsModelCohesive::synchronizeGhostFacetsConnectivity`) and
//! the facet synchronizer follows element events through
//! `ElementSynchronizer::onElementsChanged`/`onElementsRemoved`
//! ([`crate::synchronizers::substitute_elements`],
//! [`crate::synchronizers::renumber_elements`]). The design for the port's
//! parallel cohesive insertion is in `docs/ARCHITECTURE.md` ("Facet
//! synchronizer updates on insertion"): point-to-point updates along the
//! existing facet schemes only.

use std::collections::{BTreeMap, HashMap, HashSet};

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{Communicator, DataAccessor, SendMode, Synchronizer, Tag};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{
    Element, ElementType, ElementTypeMapArray, Error, GhostType, GlobalIdx, Rank, Result,
    TypeFilter,
};

use crate::mesh::Mesh;
use crate::synchronizers::ElementSynchronizer;
use crate::utils::{FacetFlipMode, SUBELEMENT_TO_ELEMENT};

/// Facet ghost exchanges (upstream `FacetSynchronizer`, an
/// `ElementSynchronizer` of the facet mesh).
pub type FacetSynchronizer = Synchronizer<Element>;

/// Tag of the facet-scheme announcement (NBX; distinct from every
/// synchronizer data tag, [`akantu_comm::synchronizer::message_tag`] ≤
/// 26 687, and from the node-scheme rebuild tag 29 000).
const FACET_SCHEME_TAG: Tag = Tag(29_100);

/// Wire code of an element type (its upstream enum value).
fn type_code(ty: ElementType) -> GlobalIdx {
    GlobalIdx::from(ty as u8)
}

fn type_of_code(code: GlobalIdx) -> Result<ElementType> {
    ElementType::ALL
        .iter()
        .copied()
        .find(|&t| type_code(t) == code)
        .ok_or_else(|| Error::Communication(format!("facet scheme: unknown element type {code}")))
}

/// Sorted global connectivity of `facet`.
fn sorted_global_connectivity(mesh_facets: &Mesh, facet: Element) -> Result<Vec<GlobalIdx>> {
    let mut g: Vec<GlobalIdx> = mesh_facets
        .element_connectivity(facet)?
        .iter()
        .map(|&n| mesh_facets.node_global_id(n))
        .collect();
    g.sort_unstable();
    Ok(g)
}

/// Local part of the facet synchronizer construction: the receive
/// schemes and the announcement message to every element-receive rank.
struct FacetSchemePlan {
    recv: BTreeMap<Rank, Vec<Element>>,
    messages: Vec<(Rank, Vec<GlobalIdx>)>,
}

/// Owner of every ghost facet bounding a received ghost element (upstream
/// `element_to_prank` of the facet synchronizer: the minimum sending rank).
fn ghost_facet_owners(
    mesh_facets: &Mesh,
    element_synchronizer: &ElementSynchronizer,
) -> Result<HashMap<Element, Rank>> {
    let rank = mesh_facets.communicator().rank();
    let mut owner: HashMap<Element, Rank> = HashMap::new();
    if element_synchronizer
        .communications
        .recv
        .values()
        .all(Vec::is_empty)
    {
        return Ok(owner);
    }
    let s2e = mesh_facets.data().element_map(SUBELEMENT_TO_ELEMENT)?;
    for (&proc, scheme) in &element_synchronizer.communications.recv {
        for &element in scheme {
            for &facet in s2e.element(element)? {
                if facet.is_null() || facet.ghost_type() != GhostType::Ghost {
                    continue;
                }
                let r = owner.entry(facet).or_insert(rank);
                // upstream: `(proc < facet_rank) || (facet_rank == rank)`
                if proc < *r || *r == rank {
                    *r = proc;
                }
            }
        }
    }
    Ok(owner)
}

fn plan_facet_schemes(
    mesh_facets: &Mesh,
    element_synchronizer: &ElementSynchronizer,
) -> Result<FacetSchemePlan> {
    let owner = ghost_facet_owners(mesh_facets, element_synchronizer)?;
    let mut recv: BTreeMap<Rank, Vec<Element>> = BTreeMap::new();
    let mut messages = Vec::with_capacity(element_synchronizer.communications.recv.len());
    let s2e = if owner.is_empty() {
        None
    } else {
        Some(mesh_facets.data().element_map(SUBELEMENT_TO_ELEMENT)?)
    };
    for (&proc, scheme) in &element_synchronizer.communications.recv {
        let facet_scheme = recv.entry(proc).or_default();
        let mut checked: HashSet<Element> = HashSet::new();
        // one entry per facet, in receive-scheme order: [type code, sorted
        // global connectivity…]
        let mut msg = Vec::new();
        if let Some(s2e) = s2e {
            for &element in scheme {
                for &facet in s2e.element(element)? {
                    if facet.is_null()
                        || facet.ghost_type() != GhostType::Ghost
                        || owner.get(&facet) != Some(&proc)
                        || !checked.insert(facet)
                    {
                        continue;
                    }
                    facet_scheme.push(facet);
                    msg.push(type_code(facet.ty()));
                    msg.extend(sorted_global_connectivity(mesh_facets, facet)?);
                }
            }
        }
        messages.push((proc, msg));
    }
    Ok(FacetSchemePlan { recv, messages })
}

/// Send schemes from the announcements received (rank order): every
/// announced connectivity matched with the first local facet of that type
/// with the same sorted global connectivity (upstream `std::find`).
fn match_announcements(
    mesh_facets: &Mesh,
    received: BTreeMap<Rank, Vec<GlobalIdx>>,
) -> Result<BTreeMap<Rank, Vec<Element>>> {
    let mut index: HashMap<ElementType, HashMap<Vec<GlobalIdx>, usize>> = HashMap::new();
    let mut send = BTreeMap::new();
    for (proc, msg) in received {
        let scheme: &mut Vec<Element> = send.entry(proc).or_default();
        let mut i = 0usize;
        while i < msg.len() {
            let ty = type_of_code(msg[i])?;
            let nnpe = ty.nb_nodes_per_element();
            let c = msg.get(i + 1..i + 1 + nnpe).ok_or_else(|| {
                Error::Communication(format!(
                    "facet scheme: truncated announcement from rank {proc}"
                ))
            })?;
            let local = match index.entry(ty) {
                std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                std::collections::hash_map::Entry::Vacant(v) => {
                    let mut m: HashMap<Vec<GlobalIdx>, usize> = HashMap::new();
                    for f in 0..mesh_facets.nb_element(ty, GhostType::NotGhost) {
                        let el = Element::try_new(ty, f, GhostType::NotGhost)?;
                        m.entry(sorted_global_connectivity(mesh_facets, el)?)
                            .or_insert(f);
                    }
                    v.insert(m)
                }
            };
            match local.get(c) {
                Some(&f) => scheme.push(Element::try_new(ty, f, GhostType::NotGhost)?),
                None => {
                    return Err(Error::Mesh(format!(
                        "no local facet found to send to proc {proc} corresponding to {c:?}"
                    )));
                }
            }
            i += 1 + nnpe;
        }
    }
    Ok(send)
}

/// Upstream `FacetSynchronizer(mesh_facets, element_synchronizer, id)`
/// (`facet_synchronizer.cc:35-196`) as phases of the enclosing scope `ph`:
/// the receive schemes and announcements are computed locally (agreed),
/// the announcements travel by NBX, the send schemes are matched locally
/// (agreed). `mesh_facets` must be the facet mesh of the mesh whose element
/// synchronizer is given (its `subelement_to_element` rows index the
/// parent's elements). Point-to-point only (no all-gather).
pub fn build_facet_synchronizer_in(
    ph: &mut Phases,
    mesh_facets: &Mesh,
    element_synchronizer: &ElementSynchronizer,
    id: impl Into<String>,
) -> Result<FacetSynchronizer> {
    let comm = mesh_facets.communicator().clone();
    let mut sync = Synchronizer::new(id, comm.clone());
    if comm.size() == 1 {
        // upstream builds the (empty) schemes of a one-rank distributed
        // mesh without communicating; so does the agreement-free path here
        ph.check("facet_scheme")?;
        sync.communications.recv = element_synchronizer
            .communications
            .recv
            .keys()
            .map(|&r| (r, Vec::new()))
            .collect();
        return Ok(sync);
    }
    let prepared = plan_facet_schemes(mesh_facets, element_synchronizer);
    let (recv, send) = ph.run(
        "facet_scheme",
        prepared,
        |c: &Communicator, plan: FacetSchemePlan| {
            let mut sends = Vec::with_capacity(plan.messages.len());
            for (p, msg) in plan.messages {
                sends.push(c.isend_mode(msg, p, FACET_SCHEME_TAG, SendMode::Rendezvous)?);
            }
            let mut received: BTreeMap<Rank, Vec<GlobalIdx>> = BTreeMap::new();
            let mut duplicate = None;
            c.receive_any_number(sends, FACET_SCHEME_TAG, |source, msg| {
                if received.insert(source, msg).is_some() {
                    duplicate.get_or_insert(source);
                }
                Ok(())
            })?;
            if let Some(source) = duplicate {
                return Ok(Err(Error::Communication(format!(
                    "facet scheme: two announcements from rank {source}"
                ))));
            }
            Ok(match_announcements(mesh_facets, received).map(|send| (plan.recv, send)))
        },
    )?;
    sync.communications.recv = recv;
    sync.communications.send = send;
    Ok(sync)
}

/// Upstream `FacetGlobalConnectivityAccessor` (`mesh.cc:163-213`): global
/// connectivities of every facet, sent by the owners to the ghosts with
/// `_smmc_facets_conn`.
#[derive(Debug)]
pub struct FacetGlobalConnectivityAccessor {
    /// Global connectivity of every block of the facet mesh.
    pub global_connectivity: ElementTypeMapArray<GlobalIdx>,
}

impl FacetGlobalConnectivityAccessor {
    /// Accessor over the current global connectivities of `mesh_facets`.
    pub fn new(mesh_facets: &Mesh) -> Result<Self> {
        Ok(Self {
            global_connectivity: crate::utils::global_connectivity(mesh_facets)?,
        })
    }
}

impl DataAccessor<Element> for FacetGlobalConnectivityAccessor {
    fn nb_data(&self, entities: &[Element], tag: SynchronizationTag) -> usize {
        if tag != SynchronizationTag::SmmcFacetsConn {
            return 0;
        }
        entities
            .iter()
            .map(|e| {
                akantu_comm::accessor::record_bytes::<GlobalIdx>(1, e.ty().nb_nodes_per_element())
            })
            .fold(0usize, usize::saturating_add)
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag == SynchronizationTag::SmmcFacetsConn {
            for &e in entities {
                buffer.pack_slice(self.global_connectivity.element(e)?);
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        entities: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag == SynchronizationTag::SmmcFacetsConn {
            for &e in entities {
                buffer.unpack_slice(self.global_connectivity.element_mut(e)?)?;
            }
        }
        Ok(())
    }
}

/// Options of the distributed facet mesh construction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DistributedFacetsOptions {
    /// Reordering of the ghost facets to their owner's node order (UD-122:
    /// corrected default `RemoteOrder`, upstream `UpstreamInverse`).
    pub flip: FacetFlipMode,
}

/// A facet mesh built for a distributed parent, not yet installed (see
/// [`create_distributed_mesh_facets_in`]).
#[derive(Debug)]
pub struct PreparedMeshFacets {
    /// The facet mesh (distributed, with its facet synchronizer).
    pub mesh_facets: Mesh,
    /// The parent's new `mesh_to_mesh_facet` (when it has
    /// `physical_names`).
    pub mesh_to_mesh_facet: Option<ElementTypeMapArray<Element>>,
    /// Revision of the parent the facet mesh was built from.
    pub parent_revision: u64,
}

/// UD-1141 (category C, upstream's refusal kept): a mesh of spatial
/// dimension 1 distributed over more than one rank has no facet mesh.
///
/// Upstream's `Mesh::initMeshFacets` calls `MeshUtils::flipFacets` on the
/// ghost facets, which looks up `subelement_to_element` of every ghost facet
/// type before it compares any facet (`mesh_utils.cc:569-570`); a 1-D facet
/// mesh has ghost `_point_1` facets and no `subelement_to_element` below its
/// facets (`buildAllFacets` builds none for points), so upstream throws
/// `debug::Exception` "No element of type (ghost:_point_1) in this
/// ElementTypeMapArray<akantu::Element> class
/// (mesh:mesh_facets:mesh_data:subelement_to_element)" on every rank, which
/// is also why its `test_cohesive` skips every 1-D case on more than one
/// process (observed with an oracle probe). The port refuses the same input with
/// an ordinary error, decided from data identical on every rank (spatial
/// dimension, communicator size) in the first agreed phase, before anything
/// is built: every rank returns the same error, the parent mesh is unchanged
/// and the communicator stays usable. A "distributed" mesh on one rank (UD-011) has
/// no ghost facet, upstream does not throw there, and neither does the port.
fn refuse_distributed_1d(mesh: &Mesh) -> Result<()> {
    let ranks = mesh.communicator().size();
    if mesh.spatial_dimension() == 1 && ranks > 1 {
        return Err(Error::Unsupported(format!(
            "create_distributed_mesh_facets: mesh '{}' has spatial dimension 1 and is \
             distributed over {ranks} ranks: upstream refuses a distributed 1-D facet mesh \
             (Mesh::initMeshFacets -> MeshUtils::flipFacets throws \"No element of type \
             (ghost:_point_1) in this ElementTypeMapArray<akantu::Element> class \
             (mesh:mesh_facets:mesh_data:subelement_to_element)\"; its test_cohesive skips \
             every 1-D case on more than one process), refused (UPSTREAM_DEFECTS UD-1141); \
             a 1-D facet mesh is supported on one rank",
            mesh.id()
        )));
    }
    Ok(())
}

/// Upstream `Mesh::initMeshFacets(id)` on a mesh distributed over several
/// ranks (`mesh.cc:233-345`) as phases of the enclosing scope `ph`
/// (collective; the parent is only read). See the module documentation.
///
/// # Errors
/// A mesh of spatial dimension 1 on more than one rank is refused with
/// [`Error::Unsupported`] on every rank (UD-1141, upstream throws), agreed
/// in the first phase; nothing is built or installed.
pub fn create_distributed_mesh_facets_in(
    ph: &mut Phases,
    mesh: &Mesh,
    id: &str,
    options: DistributedFacetsOptions,
) -> Result<PreparedMeshFacets> {
    // local construction (upstream buildAllFacets)
    let built = (|| -> Result<Mesh> {
        mesh.check_consistency()?;
        if !mesh.is_distributed() {
            return Err(Error::Mesh(format!(
                "create_distributed_mesh_facets: mesh '{}' is not distributed",
                mesh.id()
            )));
        }
        if mesh.synchronizers().element.is_none() {
            return Err(Error::Mesh(format!(
                "create_distributed_mesh_facets: mesh '{}' has no element synchronizer",
                mesh.id()
            )));
        }
        refuse_distributed_1d(mesh)?;
        if mesh.data().has_elemental(crate::utils::PHYSICAL_NAMES) {
            // checked before any collective: the installation must not fail
            // on one rank only
            crate::utils::check_mesh_to_mesh_facet_type(mesh)?;
        }
        let mut facets = crate::utils::new_facet_mesh(mesh, id)?;
        crate::utils::build_all_facets(mesh, &mut facets, mesh.spatial_dimension(), 0)?;
        Ok(facets)
    })();
    let mut facets = ph.agreed("facets:build", built, Severity::Recoverable)?;
    let es = mesh
        .synchronizers()
        .element
        .as_ref()
        .ok_or_else(|| Error::Mesh("element synchronizer vanished".into()))?;
    let sync_id = format!("{}:facet_synchronizer", facets.id());
    let sync = build_facet_synchronizer_in(ph, &facets, es, sync_id)?;
    facets.is_distributed = true;
    // owners → ghosts: global connectivities (upstream synchronizeOnce of
    // `_smmc_facets_conn`)
    let mut acc = ph.agreed(
        "facets:conn_prepare",
        FacetGlobalConnectivityAccessor::new(&facets),
        Severity::Recoverable,
    )?;
    sync.synchronize_in(ph, &mut acc, SynchronizationTag::SmmcFacetsConn)?;
    facets.synchronizers.element = Some(sync);
    // flip the ghost facets, then the physical names (barycenters of the
    // flipped facets, as upstream)
    let finished = (|| -> Result<Option<ElementTypeMapArray<Element>>> {
        crate::utils::flip_facets_local_with(
            &mut facets,
            &acc.global_connectivity,
            GhostType::Ghost,
            options.flip,
        )?;
        if mesh.data().has_elemental(crate::utils::PHYSICAL_NAMES) {
            Ok(Some(crate::utils::transfer_physical_names(
                mesh,
                &mut facets,
            )?))
        } else {
            Ok(None)
        }
    })();
    let m2f = ph.agreed("facets:finish", finished, Severity::Recoverable)?;
    Ok(PreparedMeshFacets {
        mesh_facets: facets,
        mesh_to_mesh_facet: m2f,
        parent_revision: mesh.revision(),
    })
}

/// [`create_distributed_mesh_facets_in`] in its own scope (settled):
/// collective over the mesh's communicator.
pub fn create_distributed_mesh_facets(
    mesh: &Mesh,
    id: &str,
    options: DistributedFacetsOptions,
) -> Result<PreparedMeshFacets> {
    let comm = mesh.communicator().clone();
    let mut ph = Phases::new(&comm, format!("init_mesh_facets:{}", mesh.id()));
    let r = create_distributed_mesh_facets_in(&mut ph, mesh, id, options);
    ph.settle(r)
}

/// Owner rank of every facet of dimension `d − 1` of a distributed facet
/// mesh, upstream's convention (`ElementSynchronizer::buildElementToPrank`,
/// `element_synchronizer.cc:141-153`, used by `getRank`): every facet starts
/// at this rank (`_default_value = rank`, ghost facets included), then the
/// facets of the facet synchronizer's receive scheme from `q` get `q`. A
/// ghost facet in no receive scheme therefore reports this rank.
/// Helper of tests and consumers.
pub fn facet_to_prank(mesh_facets: &Mesh) -> Result<ElementTypeMapArray<Rank>> {
    let rank = mesh_facets.communicator().rank();
    let dim = mesh_facets.spatial_dimension();
    let mut out = ElementTypeMapArray::new("facet_to_prank");
    for ghost in GhostType::ALL {
        for ty in mesh_facets.element_types(TypeFilter::new().dim(dim - 1).ghost(ghost)) {
            out.alloc(mesh_facets.nb_element(ty, ghost), 1, ty, ghost, rank)?;
        }
    }
    if let Some(fs) = &mesh_facets.synchronizers().element {
        for (&q, facets) in &fs.communications.recv {
            for &f in facets {
                if let Ok(v) = out.element_mut(f) {
                    v[0] = q;
                }
            }
        }
    }
    Ok(out)
}
