// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/group_manager.cc
// (createBoundaryGroupFromGeometry, createClusters, ClusteringFilter,
// ClusterSynchronizer), src/mesh/group_manager.hh.

//! Clusters of connected elements (upstream `GroupManager::createClusters`,
//! `createBoundaryGroupFromGeometry`; MESH-026).
//!
//! Elements of one dimension are grouped into connected components through
//! shared facets: a breadth-first search over `subelement_to_element` /
//! `element_to_subelement` of a facet mesh, started from every unseen
//! element in (ghost type, element type, index) order. Cluster `i` becomes
//! the element group `<prefix>_<i>` (replacing an existing group of that
//! name), its elements in discovery order, its nodes added without
//! duplicate check and then sorted (only the node group is optimized, as
//! upstream). Point elements are clusters by themselves.
//!
//! **Distributed meshes** (more than one rank, upstream
//! `ClusterSynchronizer`, `group_manager.cc:221-430` and the parallel
//! branches of `createClusters`, `:497-591`). Each rank first clusters what
//! it holds (local **and** ghost elements; its local clusters are
//! upstream's temporary groups `tmp_<prefix>_<i>`), then
//! [`synchronize_clusters_in`] numbers them globally and merges the
//! clusters that touch across a rank boundary:
//!
//! 1. global fragment numbers: an exclusive scan of the local cluster
//!    counts gives the rank's first number (`starting_index`), the sum the
//!    number of fragments over every rank;
//! 2. over the mesh's element synchronizer (`_gm_clusters`, owner →
//!    ghost), every owner sends the global number of the cluster of each of
//!    its elements that another rank holds as a ghost; the receiving rank
//!    records the pair (its cluster of the ghost copy, the owner's cluster)
//!    in a sorted set;
//! 3. the pairs of all ranks are all-gathered in rank order and merged into
//!    global clusters exactly as upstream (take the first remaining pair,
//!    follow the first occurrence of each fragment in the remaining pairs,
//!    removing a used pair by moving the last one into its place — upstream
//!    `Array::erase`); the fragments in no pair follow as clusters of their
//!    own in increasing number;
//! 4. global cluster `c` becomes the group `<prefix>_<c>` on **every**
//!    rank (empty where the rank holds none of it), made of the rank's
//!    local clusters it contains (increasing number), elements and nodes
//!    sorted and deduplicated (upstream `ElementGroup::append` →
//!    `optimize`); then the group names are synchronised (upstream
//!    `synchronizeGroupNames`). The returned count is the global one.
//!
//! On one rank (distributed or not) the serial path runs, as upstream.
//! Port decisions on a distributed mesh (UPSTREAM_DEFECTS UD-1040,
//! UD-1041, UD-1042): a clustering whose element dimension differs from
//! an element of the element synchronizer is refused on every rank
//! (upstream throws from `packData` in the middle of the exchange); a
//! filter that rejects an owner's element while accepting its ghost copy
//! is refused (upstream pairs the ghost's cluster with `starting_index − 1`
//! of the sender, i.e. another rank's cluster, or −1 on rank 0); no
//! temporary group is created (upstream replaces, then destroys, groups
//! named `tmp_<prefix>_<i>`).

use std::collections::{BTreeSet, VecDeque};

use akantu_comm::DataAccessor;
use akantu_comm::ReduceOp;
use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::{Phases, Severity};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Element, ElementTypeMapArray, Error, GhostType, Result, TypeFilter};

use crate::mesh::Mesh;
use crate::utils::{
    ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT, build_all_facets, new_facet_mesh,
};

/// Which facet mesh drives the connectivity search.
#[derive(Clone, Copy, Debug)]
pub enum ClusterFacets<'a> {
    /// A temporary facet mesh built for the search (upstream
    /// `createClusters(element_dimension, prefix, filter)`:
    /// `buildAllFacets(mesh, mesh_facets, d, d − 1)`).
    Build,
    /// The mesh's own facet mesh (`initMeshFacets` must have run).
    Own,
    /// Another facet mesh sharing the node numbering (upstream
    /// `createClusters(element_dimension, mesh_facets, prefix, filter)`).
    External(&'a Mesh),
}

/// Connected components (in discovery order, elements in breadth-first
/// order) of the elements of `element_dimension` accepted by `filter`.
/// Pure: the mesh is not modified.
pub fn compute_clusters(
    mesh: &Mesh,
    mesh_facets: &Mesh,
    element_dimension: usize,
    filter: &dyn Fn(Element) -> bool,
) -> Result<Vec<Vec<Element>>> {
    let mut seen: ElementTypeMapArray<bool> = ElementTypeMapArray::new("seen_elements");
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(element_dimension).ghost(ghost)) {
            let a = seen.alloc(mesh.nb_element(ty, ghost), 1, ty, ghost, false)?;
            for (e, s) in a.as_mut_slice().iter_mut().enumerate() {
                *s = !filter(Element::try_new(ty, e, ghost)?);
            }
        }
    }
    // upstream reads the adjacency per element: without any element to
    // cluster, missing adjacency data is no error (e.g. no element of the
    // dimension at all)
    let any_element = GhostType::ALL.into_iter().any(|ghost| {
        mesh.element_types(TypeFilter::new().dim(element_dimension).ghost(ghost))
            .any(|ty| {
                seen.get(ty, ghost)
                    .is_ok_and(|a| a.as_slice().iter().any(|s| !s))
            })
    });
    if !any_element {
        return Ok(Vec::new());
    }
    let s2e = if element_dimension == 0 {
        None
    } else {
        Some(mesh_facets.data().element_map(SUBELEMENT_TO_ELEMENT)?)
    };
    let e2s = if element_dimension == 0 {
        None
    } else {
        Some(mesh_facets.data().element_lists(ELEMENT_TO_SUBELEMENT)?)
    };
    // Shape of the adjacency data (user-writable mesh
    // data, never indexed unchecked): one list per facet.
    if let Some(e2s) = e2s {
        for (ty, g, a) in e2s.as_map().iter_all() {
            if a.nb_component() != 1 && !a.is_empty() {
                return Err(Error::Mesh(format!(
                    "createClusters: {ELEMENT_TO_SUBELEMENT} of {ty}:{g} has {} components, expected 1",
                    a.nb_component()
                )));
            }
        }
    }
    let mut clusters = Vec::new();
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(element_dimension).ghost(ghost)) {
            for e in 0..mesh.nb_element(ty, ghost) {
                let start = Element::try_new(ty, e, ghost)?;
                {
                    let s = &mut seen.element_mut(start)?[0];
                    if *s {
                        continue;
                    }
                    *s = true;
                }
                let mut cluster = Vec::new();
                let (Some(s2e), Some(e2s)) = (s2e, e2s) else {
                    // point elements are clusters by themselves
                    cluster.push(start);
                    clusters.push(cluster);
                    continue;
                };
                let mut queue = VecDeque::from([start]);
                while let Some(el) = queue.pop_front() {
                    cluster.push(el);
                    for &facet in s2e.element(el)? {
                        if facet.is_null() {
                            continue;
                        }
                        let neighbours = e2s.element(facet)?.first().ok_or_else(|| {
                            Error::Mesh(format!(
                                "createClusters: empty {ELEMENT_TO_SUBELEMENT} row of {facet}"
                            ))
                        })?;
                        for &check in neighbours {
                            if check.is_null() || check == el {
                                continue;
                            }
                            let s = &mut seen.element_mut(check)?[0];
                            if !*s {
                                *s = true;
                                queue.push_back(check);
                            }
                        }
                    }
                }
                clusters.push(cluster);
            }
        }
    }
    Ok(clusters)
}

/// The local clusters of `facets` (see [`compute_clusters`]).
fn local_clusters(
    mesh: &Mesh,
    element_dimension: usize,
    filter: &dyn Fn(Element) -> bool,
    facets: ClusterFacets<'_>,
) -> Result<Vec<Vec<Element>>> {
    match facets {
        ClusterFacets::Build => {
            let mut mf = new_facet_mesh(mesh, "mesh_facets_for_clusters")?;
            build_all_facets(
                mesh,
                &mut mf,
                element_dimension,
                element_dimension.saturating_sub(1),
            )?;
            compute_clusters(mesh, &mf, element_dimension, filter)
        }
        ClusterFacets::Own => {
            let mf = mesh
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("createClusters: the mesh has no facet mesh".into()))?;
            compute_clusters(mesh, mf, element_dimension, filter)
        }
        ClusterFacets::External(mf) => compute_clusters(mesh, mf, element_dimension, filter),
    }
}

/// A clash of `<name>_nodes` with a standalone node group cannot be
/// replaced (the serial path's validation, before any change).
fn validate_cluster_names(mesh: &Mesh, names: &[String]) -> Result<()> {
    for name in names {
        let ng = format!("{name}_nodes");
        if mesh.groups.node_group(&ng).is_ok() && !mesh.groups.element_group_exists(name) {
            return Err(Error::AlreadyExists {
                kind: "node group",
                id: ng,
            });
        }
    }
    Ok(())
}

/// Upstream `GroupManager::createClusters(element_dimension, prefix,
/// filter[, mesh_facets])`: creates the groups `<prefix>_<i>` and returns
/// the number of clusters (over every rank on a distributed mesh; see the
/// module docs).
///
/// # Errors
/// Inconsistent mesh, missing facet mesh, malformed adjacency, a name
/// clash with a standalone node group; on a distributed mesh also the
/// refusals of [`synchronize_clusters_in`] (agreed on every rank).
pub fn create_clusters(
    mesh: &mut Mesh,
    element_dimension: usize,
    prefix: &str,
    filter: &dyn Fn(Element) -> bool,
    facets: ClusterFacets<'_>,
) -> Result<usize> {
    if mesh.is_distributed() && mesh.communicator().size() > 1 {
        return create_clusters_distributed(mesh, element_dimension, prefix, filter, facets);
    }
    mesh.check_consistency()?;
    let clusters = local_clusters(mesh, element_dimension, filter, facets)?;
    // Validate every name first: a clash with a standalone node group
    // `<prefix>_<i>_nodes` cannot be replaced.
    let names: Vec<String> = (0..clusters.len())
        .map(|i| format!("{prefix}_{i}"))
        .collect();
    validate_cluster_names(mesh, &names)?;
    let conn = &mesh.connectivities;
    let groups = &mut mesh.groups;
    for (name, cluster) in names.iter().zip(&clusters) {
        let mut g = groups.create_element_group_replace(name, element_dimension, true)?;
        for &el in cluster {
            g.add(el, Some(&conn.element_ref(el)?), false)?;
        }
    }
    for name in &names {
        groups.element_group_mut(name)?.node_group.optimize();
    }
    Ok(clusters.len())
}

/// The distributed branch of [`create_clusters`] (more than one rank): one
/// collective scope; every refusal is agreed before the groups change.
fn create_clusters_distributed(
    mesh: &mut Mesh,
    element_dimension: usize,
    prefix: &str,
    filter: &dyn Fn(Element) -> bool,
    facets: ClusterFacets<'_>,
) -> Result<usize> {
    let comm = mesh.communicator().clone();
    let mut ph = Phases::new(&comm, format!("createClusters:{prefix}"));
    let r = (|| -> Result<usize> {
        let local = mesh
            .check_consistency()
            .and_then(|()| local_clusters(mesh, element_dimension, filter, facets));
        let local = ph.agreed("clusters:local", local, Severity::Recoverable)?;
        let global = synchronize_clusters_in(&mut ph, mesh, element_dimension, &local)?;
        let lists = global.element_lists(&local);
        let names: Vec<String> = (0..lists.len()).map(|c| format!("{prefix}_{c}")).collect();
        ph.agree(
            "clusters:names",
            validate_cluster_names(mesh, &names),
            Severity::Recoverable,
        )?;
        let created = (|| -> Result<()> {
            let conn = &mesh.connectivities;
            let groups = &mut mesh.groups;
            for (name, list) in names.iter().zip(&lists) {
                let mut g = groups.create_element_group_replace(name, element_dimension, true)?;
                for &el in list {
                    g.add(el, Some(&conn.element_ref(el)?), false)?;
                }
                // upstream `ElementGroup::append` → `optimize`: elements and
                // nodes sorted and deduplicated (a group without local
                // member stays empty)
                g.optimize();
            }
            Ok(())
        })();
        ph.agree("clusters:groups", created, Severity::Recoverable)?;
        crate::distribution::synchronize_group_names_in(&mut ph, &mut mesh.groups, 0)?;
        Ok(lists.len())
    })();
    ph.settle(r)
}

/// Upstream `ClusterSynchronizer::synchronize` result (see the module
/// docs): the global numbering of the local clusters and the global
/// clusters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlobalClusters {
    /// Global number of this rank's first local cluster (exclusive scan of
    /// the local cluster counts).
    pub starting_index: usize,
    /// Local clusters (upstream "fragments") over every rank.
    pub global_nb_fragment: usize,
    /// The global clusters in upstream order, each as the ascending global
    /// numbers of the local clusters it merges.
    pub clusters: Vec<Vec<usize>>,
}

impl GlobalClusters {
    /// Positions in `0..nb_local` of this rank's local clusters that belong
    /// to global cluster `c`, in increasing global number.
    #[must_use]
    pub fn local_members(&self, c: usize, nb_local: usize) -> Vec<usize> {
        self.clusters.get(c).map_or_else(Vec::new, |gcs| {
            gcs.iter()
                .filter_map(|&gc| {
                    gc.checked_sub(self.starting_index)
                        .filter(|&l| l < nb_local)
                })
                .collect()
        })
    }

    /// The element list of every global cluster on this rank (one per
    /// global cluster, empty where the rank holds none of it): the elements
    /// of its local clusters, sorted by (ghost type, element type, index)
    /// and deduplicated (upstream `ElementGroup::append` → `optimize`).
    #[must_use]
    pub fn element_lists(&self, local: &[Vec<Element>]) -> Vec<Vec<Element>> {
        (0..self.clusters.len())
            .map(|c| {
                let mut list: Vec<Element> = self
                    .local_members(c, local.len())
                    .into_iter()
                    .flat_map(|l| local[l].iter().copied())
                    .collect();
                list.sort_by_key(|e| element_order_key(*e));
                list.dedup();
                list
            })
            .collect()
    }
}

/// Order of an upstream optimized element group: ghost type (not ghost
/// first), element type (enum order), element index.
fn element_order_key(e: Element) -> (u8, usize, akantu_core::Idx) {
    (
        u8::from(e.ghost_type() == GhostType::Ghost),
        e.ty().index(),
        e.index(),
    )
}

/// `_gm_clusters` accessor (upstream `ClusterSynchronizer`'s
/// `getNbData` / `packData` / `unpackData`).
struct ClusterAccessor {
    /// Local cluster of every element of the clustered dimension (−1:
    /// filtered out).
    element_to_fragment: ElementTypeMapArray<i64>,
    starting_index: i64,
    distant_ids: BTreeSet<(i64, i64)>,
}

impl DataAccessor<Element> for ClusterAccessor {
    fn nb_data(&self, elements: &[Element], tag: SynchronizationTag) -> usize {
        if tag == SynchronizationTag::GmClusters {
            elements.len().saturating_mul(std::mem::size_of::<i64>())
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
        if tag != SynchronizationTag::GmClusters {
            return Ok(());
        }
        for &el in elements {
            let f = self.element_to_fragment.element(el)?[0];
            // upstream packs `f + starting_index` also for a filtered element
            // (f = −1); the port sends −1 and the receiver refuses the
            // inconsistent filter (UD-1041)
            let v: i64 = if f < 0 { -1 } else { f + self.starting_index };
            buffer.pack(&v);
        }
        Ok(())
    }

    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != SynchronizationTag::GmClusters {
            return Ok(());
        }
        for &el in elements {
            let distant: i64 = buffer.unpack()?;
            let local = self.element_to_fragment.element(el)?[0];
            if local == -1 {
                continue;
            }
            if distant < 0 {
                return Err(Error::invalid(format!(
                    "createClusters on a distributed mesh: the filter accepts the ghost copy \
                     {el} but rejects the owner's element (upstream pairs the ghost's cluster \
                     with another rank's cluster, UPSTREAM_DEFECTS UD-1041); refused"
                )));
            }
            self.distant_ids
                .insert((local + self.starting_index, distant));
        }
        Ok(())
    }
}

/// Upstream `ClusterSynchronizer::synchronize` (steps 1–3 of the module
/// docs) for the local clusters `local` of `mesh` (distributed over more
/// than one rank), in the caller's collective scope `ph`.
///
/// # Errors
/// (Agreed on every rank.) A mesh without element synchronizer; an element
/// of the element synchronizer whose type is not of `element_dimension`
/// (UD-1040); a filter inconsistent between an owner and a ghost copy
/// (UD-1041); exchange failures.
pub fn synchronize_clusters_in(
    ph: &mut Phases,
    mesh: &Mesh,
    element_dimension: usize,
    local: &[Vec<Element>],
) -> Result<GlobalClusters> {
    // element_to_fragment over every type of the dimension (both ghost
    // types, every kind), −1 by default; then the scheme check
    let prepared = (|| -> Result<(ElementTypeMapArray<i64>, &crate::synchronizers::ElementSynchronizer)> {
        let sync = mesh.synchronizers().element.as_ref().ok_or_else(|| {
            Error::Mesh("createClusters: distributed mesh without element synchronizer".into())
        })?;
        let mut e2f: ElementTypeMapArray<i64> = ElementTypeMapArray::new("element_to_fragment");
        for ghost in GhostType::ALL {
            for ty in mesh.element_types(TypeFilter::new().dim(element_dimension).ghost(ghost)) {
                e2f.alloc(mesh.nb_element(ty, ghost), 1, ty, ghost, -1)?;
            }
        }
        for (i, cluster) in local.iter().enumerate() {
            let i = i64::try_from(i).map_err(|_| Error::invalid("createClusters: too many clusters"))?;
            for &el in cluster {
                e2f.element_mut(el)?[0] = i;
            }
        }
        for (sr, schemes) in [
            ("send", &sync.communications.send),
            ("receive", &sync.communications.recv),
        ] {
            for (proc, scheme) in schemes {
                if let Some(el) = scheme
                    .iter()
                    .find(|el| el.ty().spatial_dimension() != element_dimension)
                {
                    return Err(Error::Unsupported(format!(
                        "createClusters({element_dimension}) on a mesh distributed over several \
                         ranks: the element synchronizer holds {el} ({sr} scheme of rank \
                         {proc}), which is not of dimension {element_dimension} (upstream \
                         throws from ClusterSynchronizer::packData, UPSTREAM_DEFECTS UD-1040)"
                    )));
                }
            }
        }
        Ok((e2f, sync))
    })();
    let (e2f, sync) = ph.agreed("clusters:prepare", prepared, Severity::Recoverable)?;
    // 1. global numbers of the local clusters
    let nb_cluster = local.len() as u64;
    let (starting_index, global_nb_fragment) = ph.collective("clusters:numbering", |c| {
        let start = c.exclusive_scan_sum_u64(nb_cluster)?;
        let mut t = [nb_cluster];
        c.all_reduce(&mut t, ReduceOp::Sum)?;
        Ok((start, t[0]))
    })?;
    let to_usize = |v: u64| {
        usize::try_from(v).map_err(|_| Error::invalid("createClusters: cluster count overflow"))
    };
    let start_i = i64::try_from(starting_index)
        .map_err(|_| Error::invalid("createClusters: cluster count overflow"));
    let start_i = ph.agreed("clusters:start", start_i, Severity::Recoverable)?;
    // 2. owner -> ghost cluster numbers, local pairs
    let mut acc = ClusterAccessor {
        element_to_fragment: e2f,
        starting_index: start_i,
        distant_ids: BTreeSet::new(),
    };
    sync.synchronize_in(ph, &mut acc, SynchronizationTag::GmClusters)?;
    // 3. all pairs in rank order, merged
    let flat: Vec<i64> = acc.distant_ids.iter().flat_map(|&(a, b)| [a, b]).collect();
    let gathered = ph.collective("clusters:pairs", |c| c.all_gather_v(&flat))?;
    let pairs: Vec<[i64; 2]> = gathered
        .iter()
        .flat_map(|r| r.as_chunks::<2>().0.iter().copied())
        .collect();
    let merged = to_usize(global_nb_fragment).and_then(|n| merge_cluster_pairs(pairs, n));
    let clusters = ph.agreed("clusters:merge", merged, Severity::Recoverable)?;
    Ok(GlobalClusters {
        starting_index: to_usize(starting_index)?,
        global_nb_fragment: to_usize(global_nb_fragment)?,
        clusters,
    })
}

/// Upstream's renumbering loop of `ClusterSynchronizer::synchronize`: the
/// global clusters of `global_nb_fragment` fragments linked by `pairs` (in
/// rank order), each as its ascending fragment numbers; clusters of linked
/// fragments first (in the order their first pair is taken), then every
/// unlinked fragment alone, in increasing number. A used pair is removed by
/// moving the last pair into its place (upstream `Array::erase`), which
/// fixes the order in which the following clusters are found.
///
/// # Errors
/// A fragment number outside `0..global_nb_fragment`.
pub fn merge_cluster_pairs(
    mut pairs: Vec<[i64; 2]>,
    global_nb_fragment: usize,
) -> Result<Vec<Vec<usize>>> {
    let index = |f: i64| -> Result<usize> {
        usize::try_from(f)
            .ok()
            .filter(|&i| i < global_nb_fragment)
            .ok_or_else(|| {
                Error::invalid(format!(
                    "createClusters: fragment number {f} outside 0..{global_nb_fragment}"
                ))
            })
    };
    let mut clusters: Vec<BTreeSet<usize>> = Vec::new();
    let mut in_cluster = vec![false; global_nb_fragment];
    let mut check: VecDeque<i64> = VecDeque::new();
    while !pairs.is_empty() {
        let [first, second] = pairs.swap_remove(0);
        let mut current = BTreeSet::new();
        check.push_back(first);
        check.push_back(second);
        while let Some(&fragment) = check.front() {
            // first occurrence in the flattened remaining pairs
            let found = pairs.iter().flatten().position(|&x| x == fragment);
            if let Some(position) = found {
                let pair = position / 2;
                let other = (position + 1) % 2;
                check.push_back(pairs[pair][other]);
                pairs.swap_remove(pair);
            } else {
                check.pop_front();
                let i = index(fragment)?;
                current.insert(i);
                in_cluster[i] = true;
            }
        }
        clusters.push(current);
    }
    let mut out: Vec<Vec<usize>> = clusters
        .into_iter()
        .map(|c| c.into_iter().collect())
        .collect();
    for (c, seen) in in_cluster.iter().enumerate() {
        if !seen {
            out.push(vec![c]);
        }
    }
    Ok(out)
}

/// The clusters of [`compute_clusters`] as upstream's distributed
/// `createClusters` leaves them (more than one rank): one element list per
/// **global** cluster, sorted (see [`GlobalClusters::element_lists`]); on
/// one rank (or a mesh that is not distributed) the serial clusters in
/// discovery order. Collective in `ph` on a distributed mesh.
///
/// # Errors
/// See [`compute_clusters`] and [`synchronize_clusters_in`] (agreed).
pub fn compute_global_clusters_in(
    ph: &mut Phases,
    mesh: &Mesh,
    mesh_facets: &Mesh,
    element_dimension: usize,
    filter: &dyn Fn(Element) -> bool,
) -> Result<Vec<Vec<Element>>> {
    let local = compute_clusters(mesh, mesh_facets, element_dimension, filter);
    if !(mesh.is_distributed() && mesh.communicator().size() > 1) {
        return local;
    }
    let local = ph.agreed("clusters:local", local, Severity::Recoverable)?;
    let global = synchronize_clusters_in(ph, mesh, element_dimension, &local)?;
    Ok(global.element_lists(&local))
}

/// Upstream `GroupManager::createBoundaryGroupFromGeometry()`:
/// `createClusters(dim − 1, "boundary")` over the mesh's elements of
/// dimension − 1 (they must exist, e.g. from `buildFacets` or the mesh
/// file), with a temporary facet mesh. "Doesn't work in 1-D" upstream: in
/// 1-D the boundary points are clusters by themselves.
pub fn create_boundary_group_from_geometry(mesh: &mut Mesh) -> Result<usize> {
    let d = mesh.spatial_dimension() - 1;
    create_clusters(mesh, d, "boundary", &|_| true, ClusterFacets::Build)
}
