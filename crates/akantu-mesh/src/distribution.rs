// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh.cc (Mesh::distribute,
// distributeImpl), src/mesh_utils/mesh_utils_distribution.{hh,cc}
// (distributeMeshCentralized), src/synchronizer/{element_info_per_processor*,
// master_element_info_per_processor.cc, slave_element_info_per_processor.cc,
// node_info_per_processor.{hh,cc}}, src/mesh_utils/mesh_utils.cc
// (renumberMeshNodes), src/mesh/group_manager.cc (synchronizeGroupNames).

//! Centralised mesh distribution (MESH-030, MUTIL-010, PART-001/006…009;
//! decision record D-020, `docs/ARCHITECTURE.md`).
//!
//! The root rank holds the whole mesh and an element [`MeshPartition`];
//! every other rank starts from an empty mesh. After [`distribute`] every
//! rank holds its **local mesh**: its elements (not ghost) and one layer of
//! ghost elements (the elements of other parts sharing a node with a local
//! one), the nodes they use with their global ids, node flags (normal,
//! master, slave, pure ghost; periodic bits) and owner ranks, the element
//! and node synchronizer schemes, element and node groups, elemental and
//! nodal mesh data, and the number of global nodes (D-037: reading never
//! communicates, distribution does).
//!
//! # Equivalence with upstream
//!
//! Upstream exchanges the pieces type by type (`MasterElementInfoPerProc` /
//! `SlaveElementInfoPerProc`, then `Master/SlaveNodeInfoPerProc`), each
//! rank renumbering its own nodes and the root merging the node flags.
//! Every quantity a rank ends with is a deterministic function of the
//! global mesh and the partition, so here the **root computes every rank's
//! local mesh** with upstream's rules and scatters one package per rank:
//!
//! * element types in `elementTypes` (enum) order, every type of the
//!   partition (all dimensions); per type, local elements in global order,
//!   then ghost elements in global order (a ghost of every part listed in
//!   the element's ghost parts); empty blocks are created too;
//! * nodes numbered by first appearance over the per-type concatenation
//!   `[local elements, ghost elements]` (`renumberMeshNodes`), global ids
//!   = the root's node numbers;
//! * flags: normal (only in local elements), pure ghost (only in ghost
//!   elements), master (both); among the ranks claiming master, the lowest
//!   keeps it, the others become slaves; owner rank (`nodes_prank`) of
//!   slaves and pure ghosts = the master's rank, or the rank where the node
//!   is normal;
//! * node schemes: a master sends to every slave rank the nodes it masters
//!   there, a slave receives from the master rank — both sorted by global
//!   id; pure ghosts are in no node scheme;
//! * element schemes (types of the mesh dimension only, appended type by
//!   type): local elements in local order toward every ghost part, ghost
//!   elements in ghost order from their owner;
//! * ghost counters 1 per ghost element (every type);
//! * element groups: every element joins its groups (group-name order) in
//!   local then ghost order, without nodes; node groups: every local node
//!   whose global id belongs to the group, sorted (upstream `optimize`);
//!   group names (with node group and dimension) known on every rank:
//!   every rank's names are gathered on the root first and merged there
//!   (the root's definitions, then each rank's missing names in rank
//!   order), a non-root rank keeps its own definitions and adds the merged
//!   names it lacks (upstream `synchronizeGroupNames` /
//!   `checkAndAddGroups`);
//! * elemental mesh data per type, tags in name order, local then ghost
//!   values; nodal data in local node order; `element_to_subelement` /
//!   `subelement_to_element` recomputed on every rank
//!   (`fillElementToSubElementsData`);
//! * periodic pairs whose slave is local; a missing partner is appended as
//!   a pure ghost node (prank = its owner) that the owner sends to it
//!   (`fillPeriodicPairs`, `receiveMissingPeriodic`).
//!
//! # Collective protocol (ARCHITECTURE.md, "Agreement across ranks")
//!
//! One [`Phases`] scope: local preflight (validation, root planning, every
//! package packed) agreed with a digest of `(spatial dimension, root)`;
//! one `scatter_v` of the packages; local unpacking and construction of
//! the new mesh (a separate object: nothing of the existing mesh changes
//! before every rank succeeded), agreed; the bounding-box reduction; then
//! the commit (infallible). Rank-specific failures are therefore errors on
//! every rank, the meshes unchanged.
//!
//! # Deviations (documented)
//!
//! * Non-root meshes must be empty (no nodes, elements or mesh data) and
//!   the root mesh must have no facet mesh and no ghost elements; every
//!   element type of the root mesh must be partitioned (upstream silently
//!   drops unpartitioned types).
//! * Nodal data of periodic meshes: the appended missing nodes get their
//!   values (upstream's buffer is short of them).
//!
//! Memory: centralised distribution allocates the global mesh, the
//! partition and every rank's package on the root rank (upstream holds all
//! per-rank buffers of a type at once too); this is the documented
//! capacity limit of the path.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::{Phases, Severity, digest_bytes};
use akantu_comm::{Communicator, Synchronizer};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, GlobalIdx, Idx, NodeFlag,
    Rank, Real, Result, TypeFilter, try_idx, try_to_usize,
};

use crate::data::{MeshData, MeshDataTypeCode, MeshDataValues, NodalDataValues};
use crate::events::{MeshEvent, MeshEvents};
use crate::group::GroupManager;
use crate::mesh::{BBox, Mesh};
use crate::partition::MeshPartition;
use crate::synchronizers::MeshSynchronizers;
use crate::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT};

pub use crate::partition::{
    MeshDataPartition, MeshPartitioner, PartitionMetadata, PartitionerKind, PrescribedPartition,
    RcbPartitioner,
};

const NG: GhostType = GhostType::NotGhost;
const GH: GhostType = GhostType::Ghost;

/// Distribute a mesh held by `root` over the mesh's communicator (upstream
/// `Mesh::distribute(partition, communicator)` with the root-side
/// partition computed by `partitioner`; collective). `partitioner` is used
/// on the root only (`None` elsewhere is fine; required on the root when
/// the communicator has more than one rank).
pub fn distribute_with(
    mesh: &mut Mesh,
    partitioner: Option<&mut dyn MeshPartitioner>,
    root: Rank,
) -> Result<MeshEvents> {
    let comm = mesh.communicator().clone();
    let partition = if comm.rank() == root && comm.size() > 1 {
        match partitioner {
            Some(p) => Some(p.partition(mesh, comm.size())),
            None => Some(Err(Error::InvalidArgument(
                "distribute: the root needs a partitioner".into(),
            ))),
        }
    } else {
        None
    };
    match partition {
        Some(Err(e)) => distribute_impl(mesh, Err(e), root),
        Some(Ok(p)) => distribute_impl(mesh, Ok(Some(&p)), root),
        None => distribute_impl(mesh, Ok(None), root),
    }
}

/// Distribute a mesh held by `root` with an already computed partition
/// (upstream `Mesh::distribute(partition, communicator)`; collective). The
/// partition is read on the root only.
pub fn distribute(
    mesh: &mut Mesh,
    partition: Option<&MeshPartition>,
    root: Rank,
) -> Result<MeshEvents> {
    distribute_impl(mesh, Ok(partition), root)
}

fn distribute_impl(
    mesh: &mut Mesh,
    partition: Result<Option<&MeshPartition>>,
    root: Rank,
) -> Result<MeshEvents> {
    let comm = mesh.communicator().clone();
    let size = comm.size();
    let rank = comm.rank();
    let mut ph = Phases::new(&comm, format!("distribute:{}", mesh.id()));
    let r = (|| -> Result<MeshEvents> {
        // ---- group names (upstream synchronizeGroupNames):
        // every rank's names gathered on the root, merged there
        // (root's definitions first, then each rank's missing names in rank
        // order); the merged list travels in the packages ----------------
        let names = if size > 1 {
            let mine = GroupNames::of(mesh.groups()).to_bytes();
            let gathered = ph.run("group-names", Ok(mine), |c, mine| {
                Ok(match c.gather_v(&mine, root)? {
                    Some(per_rank) => GroupNames::merged(mesh.groups(), &per_rank, root).map(Some),
                    None => Ok(None),
                })
            })?;
            gathered.unwrap_or_default()
        } else {
            GroupNames::default()
        };
        // ---- local preflight (root plans and packs every package) --------
        let prepared = partition.and_then(|p| preflight(mesh, p, root, size, rank, &names));
        let digest = digest_bytes(
            digest_bytes(
                0x6d65_7368,
                &(mesh.spatial_dimension() as u64).to_le_bytes(),
            ),
            &root.to_le_bytes(),
        );
        let packages = match prepared {
            Ok(p) => {
                ph.agree_digest("preflight", Ok(()), digest, Severity::Recoverable)?;
                p
            }
            Err(e) => {
                let r = ph.agree_digest("preflight", Err(e), digest, Severity::Recoverable);
                return Err(r.err().unwrap_or_else(|| Error::Mesh("lost error".into())));
            }
        };
        // ---- scatter, then local construction (agreed) --------------------
        let (new_mesh, local_bbox) = if size == 1 {
            let mut m = take_single(mesh);
            m.nb_global_nodes = m.nb_nodes();
            let b = m.local_bounding_box();
            (m, b)
        } else {
            let mine = ph.collective("scatter", |c| c.scatter_v(packages.as_deref(), root))?;
            let built = build_local_mesh(mesh, &comm, mine, rank == root);
            let m = ph.agreed("build", built, Severity::Recoverable)?;
            let b = m.local_bounding_box();
            (m, b)
        };
        // ---- global bounding box (upstream computeBoundingBox, allSum) ----
        let mut lower = local_bbox.lower.clone();
        let mut upper = local_bbox.upper.clone();
        ph.collective("bbox", |c| {
            crate::mesh::all_sum_bounds(c, &mut lower, &mut upper)
        })?;
        // ---- commit (infallible) -------------------------------------------
        let mut new_mesh = new_mesh;
        new_mesh.bbox_local = local_bbox;
        new_mesh.bbox = BBox { lower, upper };
        new_mesh.is_distributed = true;
        new_mesh.revision = mesh.revision + 1;
        new_mesh.id = mesh.id.clone();
        new_mesh.communicator = comm.clone();
        *mesh = new_mesh;
        Ok(MeshEvents(vec![MeshEvent::MeshIsDistributed {
            origin: "Mesh::distribute".into(),
        }]))
    })();
    ph.settle(r)
}

/// One rank: the mesh keeps its content; distribution only creates empty
/// synchronizers and marks it distributed (upstream: no
/// `distributeMeshCentralized` call on one rank).
fn take_single(mesh: &mut Mesh) -> Mesh {
    let mut m = std::mem::replace(
        mesh,
        Mesh::with_communicator(
            mesh.spatial_dimension,
            mesh.id.clone(),
            mesh.communicator.clone(),
        )
        .unwrap_or_else(|_| unreachable_mesh()),
    );
    let id = m.id.clone();
    let comm = m.communicator.clone();
    m.synchronizers = MeshSynchronizers {
        element: Some(Synchronizer::new(
            format!("{id}:element_synchronizer"),
            comm.clone(),
        )),
        node: Some(Synchronizer::new(format!("{id}:node_synchronizer"), comm)),
        periodic_node: m.synchronizers.periodic_node.take(),
    };
    m
}

fn unreachable_mesh() -> Mesh {
    // `Mesh::with_communicator` only fails for an invalid dimension, which
    // an existing mesh cannot have.
    Mesh::new(1, "unreachable").unwrap_or_else(|_| std::process::abort())
}

// ------------------------------------------------------------------ preflight

/// Local validation; on the root, the packed package of every rank.
fn preflight(
    mesh: &Mesh,
    partition: Option<&MeshPartition>,
    root: Rank,
    size: usize,
    rank: Rank,
    names: &GroupNames,
) -> Result<Option<Vec<Vec<u8>>>> {
    if root < 0 || usize::try_from(root).map_or(true, |r| r >= size) {
        return Err(Error::InvalidArgument(format!(
            "distribute: root {root} outside 0..{size}"
        )));
    }
    if mesh.is_distributed() {
        return Err(Error::Mesh(
            "distribute: the mesh is already distributed".into(),
        ));
    }
    if mesh.mesh_facets().is_some() {
        return Err(Error::Mesh(
            "distribute: the mesh has a facet mesh; distribute first, then build the facets".into(),
        ));
    }
    if mesh.nb_element_filtered(TypeFilter::new().ghost(GH)) > 0 {
        return Err(Error::Mesh(
            "distribute: the mesh already has ghost elements".into(),
        ));
    }
    mesh.check_consistency()?;
    if size == 1 {
        return Ok(None);
    }
    if rank != root {
        if mesh.nb_nodes() > 0 || mesh.nb_element_filtered(TypeFilter::new()) > 0 {
            return Err(Error::Mesh(format!(
                "distribute: rank {rank} holds a non-empty mesh; only the root reads the mesh"
            )));
        }
        if !mesh.data().is_empty() {
            return Err(Error::Mesh(format!(
                "distribute: rank {rank} holds mesh data; only the root reads the mesh"
            )));
        }
        return Ok(None);
    }
    let partition = partition
        .ok_or_else(|| Error::InvalidArgument("distribute: the root needs a partition".into()))?;
    if partition.nb_parts != size {
        return Err(Error::InvalidArgument(format!(
            "distribute: the number of partitions does not match the number of processors: {} \
             != {size}",
            partition.nb_parts
        )));
    }
    let plan = Plan::new(mesh, partition, size)?;
    (0..size)
        .map(|p| plan.package(mesh, p, names))
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

// ----------------------------------------------------------------- root plan

/// Per-type distribution of the elements (root).
struct TypeSplit {
    ty: ElementType,
    /// Global indices of the local elements of each rank.
    local: Vec<Vec<usize>>,
    /// Global indices of the ghost elements of each rank.
    ghost: Vec<Vec<usize>>,
}

/// Local node numbering and flags of one rank (root).
struct RankNodes {
    /// Global id of each local node.
    globals: Vec<Idx>,
    /// Renumbered `(local, ghost)` connectivity per type (same order as
    /// `Plan::types`).
    conns: Vec<(Vec<Idx>, Vec<Idx>)>,
    /// Node flags (sharing bits; periodic bits added later).
    flags: Vec<NodeFlag>,
    /// Global id → local node (first occurrence, upstream `getNodeLocalId`).
    index: HashMap<Idx, Idx>,
}

/// Everything the root computes before packing.
struct Plan {
    types: Vec<TypeSplit>,
    ranks: Vec<RankNodes>,
    /// Owner rank of every global node.
    owner: Vec<Rank>,
    /// Pairs `(slave rank, global node)` of every master rank, in global
    /// order then rank order (upstream `nodes_to_send_per_proc`).
    master_info: Vec<Vec<(Rank, Idx)>>,
    /// Periodic: per rank, `(slave global, master global)` pairs and the
    /// missing global nodes appended to it.
    periodic: Option<Vec<PeriodicRank>>,
    /// Parts of every element, per type (same order as `types`).
    parts: Vec<Vec<Rank>>,
    /// Ghost parts of every element, per type.
    ghost_rows: Vec<crate::partition::PartCsr>,
    dim: usize,
    nb_global_nodes: usize,
    /// Per type (same order as `types`): the element groups of every
    /// element ([`element_to_groups`]), built once for all packages.
    element_groups: Vec<Csr<u32>>,
    /// Names of the root's node groups and, per global node, the indices
    /// (into the names) of the node groups listing it, as often as listed.
    node_group_names: Vec<String>,
    node_groups: Csr<u32>,
}

/// Rows of values, one per index, in one array.
#[derive(Default)]
struct Csr<T> {
    offsets: Vec<usize>,
    values: Vec<T>,
}

impl<T: Copy> Csr<T> {
    /// From `(row, value)` pairs in the order the values are listed per
    /// row; rows `0..nb_rows`.
    fn from_pairs(nb_rows: usize, pairs: &[(usize, T)]) -> Result<Self> {
        let mut offsets = vec![0usize; nb_rows + 1];
        for &(r, _) in pairs {
            *offsets.get_mut(r + 1).ok_or(Error::OutOfRange {
                what: "group entry",
                index: r,
                len: nb_rows,
            })? += 1;
        }
        for i in 0..nb_rows {
            offsets[i + 1] += offsets[i];
        }
        let mut fill = offsets.clone();
        let mut values = Vec::with_capacity(pairs.len());
        let mut slots: Vec<Option<T>> = vec![None; pairs.len()];
        for &(r, v) in pairs {
            slots[fill[r]] = Some(v);
            fill[r] += 1;
        }
        values.extend(slots.into_iter().flatten());
        Ok(Self { offsets, values })
    }

    fn row(&self, r: usize) -> &[T] {
        match (self.offsets.get(r), self.offsets.get(r + 1)) {
            (Some(&a), Some(&b)) => &self.values[a..b],
            _ => &[],
        }
    }
}

#[derive(Default)]
struct PeriodicRank {
    pairs: Vec<(Idx, Idx)>,
    missing: Vec<Idx>,
}

impl Plan {
    fn new(mesh: &Mesh, partition: &MeshPartition, size: usize) -> Result<Self> {
        let dim = mesh.spatial_dimension();
        let nb_global_nodes = mesh.nb_nodes();
        let mut types = Vec::new();
        let mut all_parts = Vec::new();
        let mut ghost_rows = Vec::new();
        for ty in mesh.element_types(TypeFilter::new()) {
            if !partition.has_partitions(ty) {
                return Err(Error::Mesh(format!(
                    "distribute: element type {ty} is not partitioned (upstream silently drops it)"
                )));
            }
            let parts = partition.partition(ty)?;
            let ghost = partition.ghost_partition(ty)?;
            let ne = mesh.nb_element(ty, NG);
            if parts.len() != ne || ghost.nb_rows() != ne {
                return Err(Error::DimensionMismatch {
                    context: "distribute: partition of a type",
                    expected: ne,
                    got: parts.len(),
                });
            }
            let mut split = TypeSplit {
                ty,
                local: vec![Vec::new(); size],
                ghost: vec![Vec::new(); size],
            };
            for (el, &p) in parts.iter().enumerate() {
                split.local[rank_index(p, size)?].push(el);
            }
            for (el, &own) in parts.iter().enumerate() {
                for &q in ghost.row(el) {
                    let qi = rank_index(q, size)?;
                    if q == own {
                        return Err(Error::Mesh(format!(
                            "distribute: {ty} element {el} lists its own part {q} as a ghost part"
                        )));
                    }
                    split.ghost[qi].push(el);
                }
            }
            types.push(split);
            all_parts.push(parts.to_vec());
            ghost_rows.push(ghost.clone());
        }
        // per-rank renumbering and flags (upstream renumberMeshNodes,
        // fillNodesType)
        let mut ranks = Vec::with_capacity(size);
        for p in 0..size {
            let mut globals: Vec<Idx> = Vec::new();
            let mut conns = Vec::with_capacity(types.len());
            for t in &types {
                let conn = mesh.connectivity(t.ty, NG)?;
                let nnpe = t.ty.nb_nodes_per_element();
                let mut buf = Vec::with_capacity((t.local[p].len() + t.ghost[p].len()) * nnpe);
                for &el in t.local[p].iter().chain(&t.ghost[p]) {
                    buf.extend_from_slice(conn.tuple(el));
                }
                let (l, g) = crate::utils::renumber_mesh_nodes(
                    &mut buf,
                    t.local[p].len(),
                    t.ghost[p].len(),
                    t.ty,
                    &mut globals,
                )?;
                conns.push((l, g));
            }
            let n = globals.len();
            let mut set = vec![0u8; n];
            for (l, g) in &conns {
                for &v in l {
                    set[try_to_usize(v)?] |= 1;
                }
                for &v in g {
                    set[try_to_usize(v)?] |= 2;
                }
            }
            let flags = set
                .iter()
                .map(|&s| match s {
                    1 => Ok(NodeFlag::NORMAL),
                    2 => Ok(NodeFlag::PURE_GHOST),
                    3 => Ok(NodeFlag::MASTER),
                    _ => Err(Error::Mesh("distribute: node in no element".into())),
                })
                .collect::<Result<Vec<_>>>()?;
            let mut index = HashMap::with_capacity(n);
            for (l, &g) in globals.iter().enumerate() {
                index.entry(g).or_insert(try_idx(l)?);
            }
            ranks.push(RankNodes {
                globals,
                conns,
                flags,
                index,
            });
        }
        // masters: the lowest rank claiming it (upstream multimap order)
        let mut owner = vec![-1 as Rank; nb_global_nodes];
        let mut candidates: Vec<(Idx, Rank, usize)> = Vec::new();
        for (p, rn) in ranks.iter().enumerate() {
            let pr = Rank::try_from(p).map_err(|_| Error::invalid("rank overflow"))?;
            for (l, (&g, &f)) in rn.globals.iter().zip(&rn.flags).enumerate() {
                if f == NodeFlag::MASTER {
                    candidates.push((g, pr, l));
                } else if f == NodeFlag::NORMAL {
                    owner[try_to_usize(g)?] = pr;
                }
            }
        }
        candidates.sort_by_key(|c| c.0); // stable: rank, then local order
        let mut master_info: Vec<Vec<(Rank, Idx)>> = vec![Vec::new(); size];
        let mut i = 0;
        while i < candidates.len() {
            let g = candidates[i].0;
            let master = candidates[i].1;
            owner[try_to_usize(g)?] = master;
            let mut j = i;
            while j < candidates.len() && candidates[j].0 == g {
                let (_, p, l) = candidates[j];
                if p != master {
                    ranks[rank_index(p, size)?].flags[l] = NodeFlag::SLAVE;
                    master_info[rank_index(master, size)?].push((p, g));
                }
                j += 1;
            }
            i = j;
        }
        if let Some(bad) = ranks
            .iter()
            .flat_map(|r| r.globals.iter())
            .find(|&&g| owner[usize::try_from(g).unwrap_or(0)] < 0)
        {
            return Err(Error::Mesh(format!(
                "distribute: global node {bad} has no owner (inconsistent partition)"
            )));
        }
        // periodicity (upstream synchronizePeriodicity / fillPeriodicPairs)
        let periodic = if mesh.is_periodic() {
            let mut out = Vec::with_capacity(size);
            for rn in &ranks {
                let mut pr = PeriodicRank::default();
                let mut known: HashMap<Idx, ()> = rn.index.keys().map(|&g| (g, ())).collect();
                for &g in &rn.globals {
                    if mesh.node_flag(g).bits() & NodeFlag::PERIODIC_MASK.bits()
                        == NodeFlag::PERIODIC_SLAVE.bits()
                    {
                        let m = mesh.periodic_master(g).ok_or_else(|| {
                            Error::Mesh(format!("distribute: periodic slave {g} without master"))
                        })?;
                        pr.pairs.push((g, m));
                    }
                }
                for &(s, m) in &pr.pairs {
                    for x in [s, m] {
                        if known.insert(x, ()).is_none() {
                            pr.missing.push(x);
                        }
                    }
                }
                out.push(pr);
            }
            Some(out)
        } else {
            None
        };
        let element_groups = types
            .iter()
            .map(|t| element_to_groups(mesh, t.ty))
            .collect::<Result<Vec<_>>>()?;
        let mut node_group_names = Vec::new();
        let mut pairs: Vec<(usize, u32)> = Vec::new();
        for (gi, g) in mesh.groups().node_groups().enumerate() {
            let gi = u32::try_from(gi).map_err(|_| Error::invalid("too many node groups"))?;
            node_group_names.push(g.name().to_owned());
            for n in g.node_list() {
                pairs.push((try_to_usize(n)?, gi));
            }
        }
        // a node listed in several groups: its row in group order
        let node_groups = Csr::from_pairs(nb_global_nodes, &pairs)?;
        Ok(Self {
            types,
            ranks,
            owner,
            master_info,
            periodic,
            parts: all_parts,
            ghost_rows,
            dim,
            nb_global_nodes,
            element_groups,
            node_group_names,
            node_groups,
        })
    }

    /// Pack the package of rank `p` (see [`build_local_mesh`] for the
    /// layout).
    fn package(&self, mesh: &Mesh, p: usize, names: &GroupNames) -> Result<Vec<u8>> {
        let rn = &self.ranks[p];
        let pr = Rank::try_from(p).map_err(|_| Error::invalid("rank overflow"))?;
        let mut b = CommunicationBuffer::new();
        b.pack(&MAGIC);
        b.pack(&(self.nb_global_nodes as u64));
        // groups: the merged names (the root's groups and the names only
        // other ranks know, by name)
        let groups = mesh.groups();
        names.pack(&mut b);
        // root element group index → index in the merged list
        let merged_index: Vec<u32> = groups
            .element_groups()
            .map(|g| {
                names
                    .element
                    .keys()
                    .position(|n| n == g.name())
                    .and_then(|i| u32::try_from(i).ok())
                    .ok_or_else(|| Error::Mesh(format!("distribute: element group {}", g.name())))
            })
            .collect::<Result<_>>()?;
        // element types
        b.pack(&(self.types.len() as u64));
        for (ti, t) in self.types.iter().enumerate() {
            let (lc, gc) = &rn.conns[ti];
            b.pack(&(t.ty.index() as u64));
            b.pack(&(t.local[p].len() as u64));
            b.pack(&(t.ghost[p].len() as u64));
            b.pack_vec(lc);
            b.pack_vec(gc);
            // ghost owners
            let parts = &self.parts[ti];
            let owners: Vec<Rank> = t.ghost[p].iter().map(|&el| parts[el]).collect();
            b.pack_vec(&owners);
            // element send scheme entries of this type (mesh dimension only)
            let mut send: BTreeMap<Rank, Vec<Idx>> = BTreeMap::new();
            if t.ty.spatial_dimension() == self.dim {
                let gp = &self.ghost_rows[ti];
                for (lel, &el) in t.local[p].iter().enumerate() {
                    for &q in gp.row(el) {
                        send.entry(q).or_default().push(try_idx(lel)?);
                    }
                }
            }
            b.pack(&(send.len() as u64));
            for (q, els) in &send {
                b.pack(q);
                b.pack_vec(els);
            }
            // element groups (names in group order, possibly repeated)
            let e2g = &self.element_groups[ti];
            let mut idx: Vec<u32> = Vec::new();
            for &el in t.local[p].iter().chain(&t.ghost[p]) {
                idx.clear();
                idx.extend(e2g.row(el).iter().map(|&gi| merged_index[gi as usize]));
                b.pack_vec(&idx);
            }
            // elemental mesh data (tags in name order, local then ghost)
            pack_elemental_data(&mut b, mesh, t.ty, &t.local[p], &t.ghost[p])?;
        }
        // nodes
        let periodic = self.periodic.as_ref().map(|v| &v[p]);
        let missing: &[Idx] = periodic.map_or(&[], |pp| pp.missing.as_slice());
        let all_globals: Vec<Idx> = rn.globals.iter().chain(missing).copied().collect();
        b.pack_vec(&rn.globals);
        b.pack_vec(missing);
        let mut coords = Vec::with_capacity(all_globals.len() * self.dim);
        for &g in &all_globals {
            coords.extend_from_slice(mesh.nodes().tuple(try_to_usize(g)?));
        }
        b.pack_vec(&coords);
        b.pack_vec(&rn.flags);
        // owner ranks of slaves and pure ghosts (missing periodic nodes:
        // their owner)
        let pranks: Vec<Rank> = all_globals
            .iter()
            .enumerate()
            .map(|(l, &g)| {
                let f = rn.flags.get(l).copied().unwrap_or(NodeFlag::PURE_GHOST);
                if f == NodeFlag::NORMAL || f == NodeFlag::MASTER {
                    -1
                } else {
                    self.owner[usize::try_from(g).unwrap_or(0)]
                }
            })
            .collect();
        b.pack_vec(&pranks);
        // node schemes
        let mut send: BTreeMap<Rank, Vec<Idx>> = BTreeMap::new();
        for &(q, g) in &self.master_info[p] {
            send.entry(q).or_default().push(g);
        }
        let mut recv: BTreeMap<Rank, Vec<Idx>> = BTreeMap::new();
        for (l, &g) in rn.globals.iter().enumerate() {
            if rn.flags[l] == NodeFlag::SLAVE {
                recv.entry(self.owner[try_to_usize(g)?])
                    .or_default()
                    .push(g);
            }
        }
        let to_local = |gs: &mut Vec<Idx>| -> Result<Vec<Idx>> {
            gs.sort_unstable();
            gs.iter()
                .map(|g| {
                    rn.index.get(g).copied().ok_or_else(|| {
                        Error::Mesh(format!("distribute: global node {g} not local to rank {p}"))
                    })
                })
                .collect()
        };
        let mut send_l: BTreeMap<Rank, Vec<Idx>> = BTreeMap::new();
        for (q, mut gs) in send {
            send_l.insert(q, to_local(&mut gs)?);
        }
        let mut recv_l: BTreeMap<Rank, Vec<Idx>> = BTreeMap::new();
        for (q, mut gs) in recv {
            recv_l.insert(q, to_local(&mut gs)?);
        }
        // periodic: missing nodes are received from their owner (appended
        // after the regular entries); this rank sends the nodes it owns to
        // the ranks missing them (upstream receiveMissingPeriodic)
        if let Some(per) = &self.periodic {
            let base = rn.globals.len();
            for (k, &g) in missing.iter().enumerate() {
                let o = self.owner[try_to_usize(g)?];
                recv_l.entry(o).or_default().push(try_idx(base + k)?);
            }
            for (q, other) in per.iter().enumerate() {
                for &g in &other.missing {
                    if self.owner[try_to_usize(g)?] == pr {
                        let l = rn.index.get(&g).copied().ok_or_else(|| {
                            Error::Mesh(format!(
                                "distribute: periodic node {g} owned by rank {p} but not local"
                            ))
                        })?;
                        send_l
                            .entry(Rank::try_from(q).map_err(|_| Error::invalid("rank"))?)
                            .or_default()
                            .push(l);
                    }
                }
            }
        }
        for m in [&send_l, &recv_l] {
            b.pack(&(m.len() as u64));
            for (q, ls) in m {
                b.pack(q);
                b.pack_vec(ls);
            }
        }
        // periodic pairs in global ids
        match periodic {
            Some(pp) => {
                b.pack(&1u8);
                let flat: Vec<Idx> = pp.pairs.iter().flat_map(|&(s, m)| [s, m]).collect();
                b.pack_vec(&flat);
            }
            None => b.pack(&0u8),
        }
        // node groups: sorted local nodes of each group (the missing
        // periodic nodes are appended after upstream filled the groups);
        // each local node is listed in the groups of its global node
        let mut lists: Vec<Vec<Idx>> = vec![Vec::new(); self.node_group_names.len()];
        for (l, &g) in rn.globals.iter().enumerate() {
            for &gi in self.node_groups.row(try_to_usize(g)?) {
                lists[gi as usize].push(try_idx(l)?);
            }
        }
        b.pack(&(names.node.len() as u64));
        for name in &names.node {
            // a name known only by other ranks: empty on the root
            let mut ls = match self.node_group_names.iter().position(|n| n == name) {
                Some(gi) => std::mem::take(&mut lists[gi]),
                None => Vec::new(),
            };
            ls.sort_unstable();
            ls.dedup();
            b.pack_vec(&ls);
        }
        // nodal data (local order, missing periodic nodes included)
        pack_nodal_data(&mut b, mesh, &all_globals)?;
        // element synchronizer receive schemes are rebuilt from the ghost
        // owners; done
        b.pack(&MAGIC);
        Ok(b.into_bytes())
    }
}

const MAGIC: u32 = 0x4d44_5354; // "MDST"

fn rank_index(p: Idx, size: usize) -> Result<usize> {
    usize::try_from(p)
        .ok()
        .filter(|&u| u < size)
        .ok_or_else(|| Error::InvalidArgument(format!("distribute: part {p} outside 0..{size}")))
}

/// Element → indices (into the name-sorted element group list) of its
/// groups, in group-name order, repeated as many times as the element is
/// listed (upstream `element_to_group`).
fn element_to_groups(mesh: &Mesh, ty: ElementType) -> Result<Csr<u32>> {
    let mut pairs: Vec<(usize, u32)> = Vec::new();
    for (gi, g) in mesh.groups().element_groups().enumerate() {
        let gi = u32::try_from(gi).map_err(|_| Error::invalid("too many element groups"))?;
        for &e in &g.elements_of(ty, NG) {
            pairs.push((try_to_usize(e)?, gi));
        }
    }
    Csr::from_pairs(mesh.nb_element(ty, NG), &pairs)
}

fn type_code_u8(c: MeshDataTypeCode) -> u8 {
    match c {
        MeshDataTypeCode::Int => 0,
        MeshDataTypeCode::UInt => 1,
        MeshDataTypeCode::Real => 2,
        MeshDataTypeCode::Bool => 3,
        MeshDataTypeCode::Element => 4,
        MeshDataTypeCode::String => 5,
        MeshDataTypeCode::ElementList => 6,
    }
}

fn type_code_of(u: u8) -> Result<MeshDataTypeCode> {
    Ok(match u {
        0 => MeshDataTypeCode::Int,
        1 => MeshDataTypeCode::UInt,
        2 => MeshDataTypeCode::Real,
        3 => MeshDataTypeCode::Bool,
        4 => MeshDataTypeCode::Element,
        5 => MeshDataTypeCode::String,
        6 => MeshDataTypeCode::ElementList,
        _ => {
            return Err(Error::Communication(format!(
                "distribute: bad data type code {u}"
            )));
        }
    })
}

/// Datasets recomputed on every rank instead of being shipped.
fn recomputed(name: &str) -> bool {
    name == ELEMENT_TO_SUBELEMENT || name == SUBELEMENT_TO_ELEMENT
}

fn pack_rows<T: Clone>(a: &Array<T>, els: &[usize], out: &mut Vec<T>) {
    for &e in els {
        out.extend_from_slice(a.tuple(e));
    }
}

fn pack_elemental_data(
    b: &mut CommunicationBuffer,
    mesh: &Mesh,
    ty: ElementType,
    local: &[usize],
    ghost: &[usize],
) -> Result<()> {
    let names: Vec<&str> = mesh
        .data()
        .tag_names(ty, NG)
        .into_iter()
        .filter(|n| !recomputed(n))
        .collect();
    b.pack(&(names.len() as u64));
    for name in names {
        let v = mesh.data().elemental(name)?;
        b.pack_str(name);
        b.pack(&type_code_u8(v.type_code()));
        b.pack(&u8::from(mesh.data().is_elemental_nodal(name)));
        macro_rules! rows {
            ($m:expr) => {{
                let a = $m.get(ty, NG)?;
                if a.len() != mesh.nb_element(ty, NG) {
                    return Err(Error::DimensionMismatch {
                        context: "distribute: elemental mesh data rows",
                        expected: mesh.nb_element(ty, NG),
                        got: a.len(),
                    });
                }
                b.pack(&(a.nb_component() as u64));
                let mut l = Vec::new();
                pack_rows(&a, local, &mut l);
                let mut g = Vec::new();
                pack_rows(&a, ghost, &mut g);
                (l, g)
            }};
        }
        match v {
            MeshDataValues::Int(m) => {
                let (l, g) = rows!(m);
                b.pack_vec(&l);
                b.pack_vec(&g);
            }
            MeshDataValues::UInt(m) => {
                let (l, g) = rows!(m);
                b.pack_vec(&l);
                b.pack_vec(&g);
            }
            MeshDataValues::Real(m) => {
                let (l, g) = rows!(m);
                b.pack_vec(&l);
                b.pack_vec(&g);
            }
            MeshDataValues::Bool(m) => {
                let (l, g) = rows!(m);
                b.pack_vec(&l);
                b.pack_vec(&g);
            }
            MeshDataValues::Element(m) => {
                let (l, g) = rows!(m);
                b.pack_vec(&l);
                b.pack_vec(&g);
            }
            MeshDataValues::String(m) => {
                let (l, g) = rows!(m);
                for s in l.iter().chain(&g) {
                    b.pack_str(s);
                }
            }
            MeshDataValues::ElementList(m) => {
                let (l, g) = rows!(m);
                for s in l.iter().chain(&g) {
                    b.pack_vec(s);
                }
            }
        }
    }
    Ok(())
}

fn pack_nodal_data(b: &mut CommunicationBuffer, mesh: &Mesh, globals: &[Idx]) -> Result<()> {
    let names: Vec<&str> = mesh.data().nodal_names().collect();
    b.pack(&(names.len() as u64));
    for name in names {
        b.pack_str(name);
        match mesh.data().nodal(name)? {
            NodalDataValues::Real(a) => {
                b.pack(&0u8);
                b.pack(&(a.nb_component() as u64));
                let mut v = Vec::with_capacity(globals.len() * a.nb_component());
                for &g in globals {
                    v.extend_from_slice(a.get_tuple(try_to_usize(g)?)?);
                }
                b.pack_vec(&v);
            }
            NodalDataValues::Int(a) => {
                b.pack(&1u8);
                b.pack(&(a.nb_component() as u64));
                let mut v = Vec::with_capacity(globals.len() * a.nb_component());
                for &g in globals {
                    v.extend_from_slice(a.get_tuple(try_to_usize(g)?)?);
                }
                b.pack_vec(&v);
            }
            NodalDataValues::UInt(a) => {
                b.pack(&2u8);
                b.pack(&(a.nb_component() as u64));
                let mut v = Vec::with_capacity(globals.len() * a.nb_component());
                for &g in globals {
                    v.extend_from_slice(a.get_tuple(try_to_usize(g)?)?);
                }
                b.pack_vec(&v);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- group names

/// Group names of a rank (upstream `fillBufferWithGroupNames`): node
/// groups, and element groups with their node group and dimension, by name
/// (upstream's `std::map` order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GroupNames {
    node: BTreeSet<String>,
    element: BTreeMap<String, (String, usize)>,
}

impl GroupNames {
    fn of(groups: &GroupManager) -> Self {
        Self {
            node: groups.node_groups().map(|g| g.name().to_owned()).collect(),
            element: groups
                .element_groups()
                .map(|g| {
                    (
                        g.name().to_owned(),
                        (g.node_group.name().to_owned(), g.group().dimension),
                    )
                })
                .collect(),
        }
    }

    fn pack(&self, b: &mut CommunicationBuffer) {
        b.pack(&(self.node.len() as u64));
        for n in &self.node {
            b.pack_str(n);
        }
        b.pack(&(self.element.len() as u64));
        for (n, (ng, d)) in &self.element {
            b.pack_str(n);
            b.pack_str(ng);
            b.pack(&(*d as u64));
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut b = CommunicationBuffer::new();
        self.pack(&mut b);
        b.into_bytes()
    }

    fn unpack(b: &mut CommunicationBuffer) -> Result<Self> {
        let mut out = Self::default();
        for _ in 0..usize_of(b.unpack::<u64>()?)? {
            out.node.insert(b.unpack_string()?);
        }
        for _ in 0..usize_of(b.unpack::<u64>()?)? {
            let n = b.unpack_string()?;
            let ng = b.unpack_string()?;
            let d = usize_of(b.unpack::<u64>()?)?;
            out.element.insert(n, (ng, d));
        }
        Ok(out)
    }

    /// Upstream `checkAndAddGroups`: add the names of `other` that do not
    /// exist yet (an existing definition is kept), node groups first.
    fn add_missing(&mut self, other: &Self) {
        for n in &other.node {
            if !self.node.contains(n) {
                self.node.insert(n.clone());
            }
        }
        for (n, def) in &other.element {
            self.element.entry(n.clone()).or_insert_with(|| def.clone());
        }
    }

    /// The root's merged names: its own, then every other rank's missing
    /// names in rank order (upstream: the root receives from ranks
    /// 1, 2, … in order).
    fn merged(root_groups: &GroupManager, per_rank: &[Vec<u8>], root: Rank) -> Result<Self> {
        let mut out = Self::of(root_groups);
        for (p, bytes) in per_rank.iter().enumerate() {
            if usize::try_from(root).ok() == Some(p) {
                continue;
            }
            let mut b = CommunicationBuffer::from_bytes(bytes.clone());
            out.add_missing(&Self::unpack(&mut b)?);
        }
        Ok(out)
    }

    /// Create the groups of `self` missing in `groups` (existing
    /// definitions kept).
    fn add_to(&self, groups: &mut GroupManager) -> Result<()> {
        for n in &self.node {
            if !groups.node_group_exists(n) {
                groups.create_node_group(n)?;
            }
        }
        for (n, (ng, d)) in &self.element {
            if !groups.element_group_exists(n) {
                groups.create_element_group_on_node_group(n, *d, ng)?;
            }
        }
        Ok(())
    }
}

/// Upstream `GroupManager::synchronizeGroupNames()` on a distributed
/// mesh, as one phase of the caller's scope `ph`: every rank's group names
/// are gathered on `root`, which adds the names it lacks (in rank order,
/// upstream `checkAndAddGroups`) and broadcasts its merged list; every rank
/// then creates the groups it lacks (existing definitions are kept). On
/// one rank nothing happens (upstream returns at once).
///
/// # Errors
/// Exchange failures; a merged definition that cannot be created (agreed).
pub(crate) fn synchronize_group_names_in(
    ph: &mut Phases,
    groups: &mut GroupManager,
    root: Rank,
) -> Result<()> {
    let (_, size) = ph.rank_size();
    if size <= 1 {
        return Ok(());
    }
    let mine = GroupNames::of(groups).to_bytes();
    let merged: Option<Vec<u8>> = ph.run("group-names:gather", Ok(mine), |c, mine| {
        Ok(match c.gather_v(&mine, root)? {
            Some(per_rank) => {
                GroupNames::merged(groups, &per_rank, root).map(|g| Some(g.to_bytes()))
            }
            None => Ok(None),
        })
    })?;
    let bytes = merged.unwrap_or_default();
    let all = ph.collective("group-names:broadcast", |c| c.broadcast_vec(&bytes, root))?;
    let added = GroupNames::unpack(&mut CommunicationBuffer::from_bytes(all))
        .and_then(|names| names.add_to(groups));
    ph.agree("group-names:add", added, Severity::Recoverable)
}

// ------------------------------------------------------------- local install

fn usize_of(v: u64) -> Result<usize> {
    usize::try_from(v).map_err(|_| Error::Communication("distribute: size overflow".into()))
}

/// Unpack this rank's package and build its local mesh (a new object; the
/// existing `mesh` provides id, dimension, communicator and — on non-root
/// ranks — its own group names).
fn build_local_mesh(
    mesh: &Mesh,
    comm: &Communicator,
    bytes: Vec<u8>,
    is_root: bool,
) -> Result<Mesh> {
    let dim = mesh.spatial_dimension();
    let id = mesh.id().to_owned();
    let mut b = CommunicationBuffer::from_bytes(bytes);
    let magic: u32 = b.unpack()?;
    if magic != MAGIC {
        return Err(Error::Communication("distribute: corrupt package".into()));
    }
    let mut m = Mesh::with_communicator(dim, id.clone(), comm.clone())?;
    m.nb_global_nodes = usize_of(b.unpack::<u64>()?)?;
    // groups (upstream synchronizeGroupNames → checkAndAddGroups): a
    // non-root rank keeps its own (empty) groups with their definitions and
    // adds the merged names it lacks; the root's own definitions are in the
    // merged list
    let received = GroupNames::unpack(&mut b)?;
    let mut groups = GroupManager::default();
    if !is_root {
        GroupNames::of(mesh.groups()).add_to(&mut groups)?;
    }
    received.add_to(&mut groups)?;
    let node_group_names: Vec<String> = received.node.iter().cloned().collect();
    let element_group_names: Vec<String> = received.element.keys().cloned().collect();
    let mut data = MeshData::default();
    let mut elem_send: BTreeMap<Rank, Vec<Element>> = BTreeMap::new();
    let mut elem_recv: BTreeMap<Rank, Vec<Element>> = BTreeMap::new();
    let nb_types = usize_of(b.unpack::<u64>()?)?;
    for _ in 0..nb_types {
        let ti = usize_of(b.unpack::<u64>()?)?;
        let ty = ElementType::from_index(ti)
            .ok_or_else(|| Error::Communication(format!("distribute: bad type index {ti}")))?;
        let nl = usize_of(b.unpack::<u64>()?)?;
        let ng = usize_of(b.unpack::<u64>()?)?;
        let nnpe = ty.nb_nodes_per_element();
        let lc: Vec<Idx> = b.unpack_vec()?;
        let gc: Vec<Idx> = b.unpack_vec()?;
        if lc.len() != nl * nnpe || gc.len() != ng * nnpe {
            return Err(Error::Communication("distribute: connectivity size".into()));
        }
        m.connectivities
            .alloc(0, nnpe, ty, NG, 0)?
            .append(&Array::from_vec(lc, nnpe)?)?;
        m.connectivities
            .alloc(0, nnpe, ty, GH, 0)?
            .append(&Array::from_vec(gc, nnpe)?)?;
        m.ghosts_counters.alloc(ng, 1, ty, GH, 1)?;
        let owners: Vec<Rank> = b.unpack_vec()?;
        if owners.len() != ng {
            return Err(Error::Communication("distribute: ghost owners size".into()));
        }
        let nb_send = usize_of(b.unpack::<u64>()?)?;
        for _ in 0..nb_send {
            let q: Rank = b.unpack()?;
            let els: Vec<Idx> = b.unpack_vec()?;
            let s = elem_send.entry(q).or_default();
            for e in els {
                s.push(Element::new(ty, e, NG));
            }
        }
        if ty.spatial_dimension() == dim {
            for (gel, &o) in owners.iter().enumerate() {
                elem_recv
                    .entry(o)
                    .or_default()
                    .push(Element::new(ty, try_idx(gel)?, GH));
            }
        }
        // element groups
        for (ghost, n) in [(NG, nl), (GH, ng)] {
            for e in 0..n {
                let names: Vec<u32> = b.unpack_vec()?;
                for gi in names {
                    let name = element_group_names.get(gi as usize).ok_or_else(|| {
                        Error::Communication("distribute: bad group index".into())
                    })?;
                    groups.element_group_mut(name)?.add(
                        Element::new(ty, try_idx(e)?, ghost),
                        None,
                        false,
                    )?;
                }
            }
        }
        unpack_elemental_data(&mut b, &mut data, ty, nl, ng)?;
    }
    // nodes
    let globals: Vec<Idx> = b.unpack_vec()?;
    let missing: Vec<Idx> = b.unpack_vec()?;
    let n = globals.len() + missing.len();
    let coords: Vec<Real> = b.unpack_vec()?;
    if coords.len() != n * dim {
        return Err(Error::Communication("distribute: coordinates size".into()));
    }
    m.nodes = Array::try_from_vec_with_len(coords, n, dim)?.into();
    m.nodes.set_id(format!("{id}:coordinates"));
    let mut flags: Vec<NodeFlag> = b.unpack_vec()?;
    if flags.len() != globals.len() {
        return Err(Error::Communication("distribute: flags size".into()));
    }
    flags.extend(std::iter::repeat_n(NodeFlag::PURE_GHOST, missing.len()));
    let all_globals: Vec<GlobalIdx> = globals
        .iter()
        .chain(&missing)
        .map(|&g| GlobalIdx::from(g))
        .collect();
    m.nodes_global_ids = Some(Array::from_vec(all_globals, 1)?);
    let pranks: Vec<Rank> = b.unpack_vec()?;
    if pranks.len() != n {
        return Err(Error::Communication("distribute: owner ranks size".into()));
    }
    for (l, &r) in pranks.iter().enumerate() {
        if r >= 0 {
            m.nodes_prank.insert(try_idx(l)?, r);
        }
    }
    let mut node_schemes: [BTreeMap<Rank, Vec<Idx>>; 2] = [BTreeMap::new(), BTreeMap::new()];
    for s in &mut node_schemes {
        let k = usize_of(b.unpack::<u64>()?)?;
        for _ in 0..k {
            let q: Rank = b.unpack()?;
            let ls: Vec<Idx> = b.unpack_vec()?;
            s.insert(q, ls);
        }
    }
    // periodic pairs (global ids) → local, flags and maps
    let periodic: u8 = b.unpack()?;
    m.nodes_flags = Array::from_vec(flags, 1)?;
    m.nodes_flags.set_id(format!("{id}:nodes_flags"));
    if periodic == 1 {
        let flat: Vec<Idx> = b.unpack_vec()?;
        let mut index: HashMap<Idx, Idx> = HashMap::new();
        for (l, &g) in globals.iter().chain(&missing).enumerate() {
            index.entry(g).or_insert(try_idx(l)?);
        }
        for pair in flat.as_chunks::<2>().0 {
            let s = *index
                .get(&pair[0])
                .ok_or_else(|| Error::Communication("distribute: periodic node".into()))?;
            let mm = *index
                .get(&pair[1])
                .ok_or_else(|| Error::Communication("distribute: periodic node".into()))?;
            add_periodic_slave(&mut m, s, mm)?;
        }
        m.is_periodic = true;
    }
    // node groups
    let nb = usize_of(b.unpack::<u64>()?)?;
    if nb != node_group_names.len() {
        return Err(Error::Communication("distribute: node group count".into()));
    }
    for name in &node_group_names {
        let ls: Vec<Idx> = b.unpack_vec()?;
        let g = groups.node_group_mut(name)?;
        for l in ls {
            g.add(l, false);
        }
        g.optimize();
    }
    unpack_nodal_data(&mut b, &mut data, n)?;
    let end: u32 = b.unpack()?;
    if end != MAGIC || b.remaining() != 0 {
        return Err(Error::Communication(
            "distribute: trailing package bytes".into(),
        ));
    }
    m.groups = groups;
    m.data = data;
    let mut es = Synchronizer::new(format!("{id}:element_synchronizer"), comm.clone());
    es.communications.send = elem_send;
    es.communications.recv = elem_recv;
    let mut ns = Synchronizer::new(format!("{id}:node_synchronizer"), comm.clone());
    let [send, recv] = node_schemes;
    ns.communications.send = send;
    ns.communications.recv = recv;
    m.synchronizers = MeshSynchronizers {
        element: Some(es),
        node: Some(ns),
        periodic_node: None,
    };
    m.check_consistency()?;
    crate::utils::fill_element_to_subelements_data(&mut m)?;
    Ok(m)
}

/// Upstream `Mesh::addPeriodicSlave` (`mesh_inline_impl.hh:611-641`): a
/// pair is registered once, the slave's master replaced, and the periodic
/// bits of both flags set (sharing bits kept).
fn add_periodic_slave(m: &mut Mesh, slave: Idx, master: Idx) -> Result<()> {
    if slave == master {
        return Ok(());
    }
    let slaves = m.periodic_master_slaves.entry(master).or_default();
    if !slaves.contains(&slave) {
        slaves.push(slave);
    }
    m.periodic_slave_master.insert(slave, master);
    let flags = m.nodes_flags.as_mut_slice();
    for (n, f) in [
        (slave, NodeFlag::PERIODIC_SLAVE),
        (master, NodeFlag::PERIODIC_MASTER),
    ] {
        let x = flags
            .get_mut(try_to_usize(n)?)
            .ok_or_else(|| Error::Communication("distribute: periodic node".into()))?;
        *x = NodeFlag((x.bits() & !NodeFlag::PERIODIC_MASK.bits()) | f.bits());
    }
    Ok(())
}

fn unpack_elemental_data(
    b: &mut CommunicationBuffer,
    data: &mut MeshData,
    ty: ElementType,
    nl: usize,
    ng: usize,
) -> Result<()> {
    let nb = usize_of(b.unpack::<u64>()?)?;
    for _ in 0..nb {
        let name = b.unpack_string()?;
        let code = type_code_of(b.unpack::<u8>()?)?;
        let nodal: u8 = b.unpack()?;
        let nbc = usize_of(b.unpack::<u64>()?)?;
        macro_rules! fill {
            ($insert:ident, $t:ty, $install:ident) => {{
                let l: Vec<$t> = b.unpack_vec()?;
                let g: Vec<$t> = b.unpack_vec()?;
                let m = data.$insert(&name)?;
                $install(m, ty, nbc, nl, ng, l, g)?;
            }};
        }
        match code {
            MeshDataTypeCode::Int => fill!(ints_mut_or_insert, i32, install_shared_rows),
            MeshDataTypeCode::UInt => fill!(uints_mut_or_insert, u32, install_shared_rows),
            MeshDataTypeCode::Real => fill!(reals_mut_or_insert, f64, install_shared_rows),
            MeshDataTypeCode::Bool => fill!(bools_mut_or_insert, bool, install_shared_rows),
            MeshDataTypeCode::Element => fill!(element_map_mut_or_insert, Element, install_rows),
            MeshDataTypeCode::String => {
                let mut l = Vec::with_capacity(nl * nbc);
                for _ in 0..nl * nbc {
                    l.push(b.unpack_string()?);
                }
                let mut g = Vec::with_capacity(ng * nbc);
                for _ in 0..ng * nbc {
                    g.push(b.unpack_string()?);
                }
                let m = data.strings_mut_or_insert(&name)?;
                install_rows(m, ty, nbc, nl, ng, l, g)?;
            }
            MeshDataTypeCode::ElementList => {
                let mut l = Vec::with_capacity(nl * nbc);
                for _ in 0..nl * nbc {
                    l.push(b.unpack_vec::<Element>()?);
                }
                let mut g = Vec::with_capacity(ng * nbc);
                for _ in 0..ng * nbc {
                    g.push(b.unpack_vec::<Element>()?);
                }
                let m = data.element_lists_mut_or_insert(&name)?;
                install_rows(m, ty, nbc, nl, ng, l, g)?;
            }
        }
        if nodal != 0 {
            data.set_elemental_nodal(&name, true);
        }
    }
    Ok(())
}

fn install_rows<T: Clone + Default>(
    m: &mut ElementTypeMapArray<T>,
    ty: ElementType,
    nbc: usize,
    nl: usize,
    ng: usize,
    l: Vec<T>,
    g: Vec<T>,
) -> Result<()> {
    if l.len() != nl * nbc || g.len() != ng * nbc {
        return Err(Error::Communication("distribute: mesh data size".into()));
    }
    for (ghost, v, n) in [(NG, l, nl), (GH, g, ng)] {
        let a = m.alloc(0, nbc, ty, ghost, T::default())?;
        *a = {
            let mut x = Array::try_from_vec_with_len(v, n, nbc)?;
            x.set_id(a.id().to_owned());
            x
        };
    }
    Ok(())
}

/// [`install_rows`] for an exportable dataset.
fn install_shared_rows<T: Clone + Default>(
    m: &mut akantu_core::SharedElementTypeMapArray<T>,
    ty: ElementType,
    nbc: usize,
    nl: usize,
    ng: usize,
    l: Vec<T>,
    g: Vec<T>,
) -> Result<()> {
    if l.len() != nl * nbc || g.len() != ng * nbc {
        return Err(Error::Communication("distribute: mesh data size".into()));
    }
    for (ghost, v, n) in [(NG, l, nl), (GH, g, ng)] {
        let mut a = m.alloc(0, nbc, ty, ghost, T::default())?;
        let mut x = Array::try_from_vec_with_len(v, n, nbc)?;
        x.set_id(a.id().to_owned());
        a.replace(x);
    }
    Ok(())
}

fn unpack_nodal_data(b: &mut CommunicationBuffer, data: &mut MeshData, n: usize) -> Result<()> {
    let nb = usize_of(b.unpack::<u64>()?)?;
    for _ in 0..nb {
        let name = b.unpack_string()?;
        let code: u8 = b.unpack()?;
        let nbc = usize_of(b.unpack::<u64>()?)?;
        match code {
            0 => {
                let v: Vec<Real> = b.unpack_vec()?;
                if v.len() != n * nbc {
                    return Err(Error::Communication("distribute: nodal data size".into()));
                }
                data.insert_nodal(
                    name,
                    NodalDataValues::Real(Array::try_from_vec_with_len(v, n, nbc)?),
                );
            }
            1 => {
                let v: Vec<akantu_core::Int> = b.unpack_vec()?;
                if v.len() != n * nbc {
                    return Err(Error::Communication("distribute: nodal data size".into()));
                }
                data.insert_nodal(
                    name,
                    NodalDataValues::Int(Array::try_from_vec_with_len(v, n, nbc)?),
                );
            }
            2 => {
                let v: Vec<akantu_core::UInt> = b.unpack_vec()?;
                if v.len() != n * nbc {
                    return Err(Error::Communication("distribute: nodal data size".into()));
                }
                data.insert_nodal(
                    name,
                    NodalDataValues::UInt(Array::try_from_vec_with_len(v, n, nbc)?),
                );
            }
            _ => {
                return Err(Error::Communication(
                    "distribute: bad nodal data code".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Element → owner rank of every element of the mesh dimension (upstream
/// `ElementSynchronizer::buildElementToPrank`): this rank for not-ghost
/// elements, the sending rank of the element synchronizer's receive scheme
/// for ghost elements (`-1` for a ghost element in no scheme).
pub fn element_to_prank(mesh: &Mesh) -> Result<ElementTypeMapArray<Rank>> {
    let rank = mesh.communicator().rank();
    let dim = mesh.spatial_dimension();
    let mut out = ElementTypeMapArray::new("element_to_prank");
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().dim(dim).ghost(ghost)) {
            let fill = if ghost == NG { rank } else { -1 };
            out.alloc(mesh.nb_element(ty, ghost), 1, ty, ghost, fill)?;
        }
    }
    if let Some(es) = &mesh.synchronizers().element {
        for (&q, els) in &es.communications.recv {
            for el in els {
                if let Ok(v) = out.element_mut(*el) {
                    v[0] = q;
                }
            }
        }
    }
    Ok(out)
}

/// Global node ids of every node set (helper of the tests and probes):
/// `(local node, global id)` sorted by global id.
#[must_use]
pub fn sorted_global_nodes(mesh: &Mesh) -> Vec<(Idx, GlobalIdx)> {
    let mut v: Vec<(Idx, GlobalIdx)> = (0..mesh.nb_nodes())
        .filter_map(|n| try_idx(n).ok())
        .map(|n| (n, mesh.node_global_id(n)))
        .collect();
    v.sort_by_key(|x| x.1);
    v
}
