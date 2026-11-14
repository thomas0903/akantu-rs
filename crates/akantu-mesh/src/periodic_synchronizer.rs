// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/periodic_node_synchronizer.hh,
// periodic_node_synchronizer.cc, periodic_node_synchronizer_tmpl.hh;
// src/mesh/mesh_periodic.cc (distributed makePeriodic, updatePeriodicSynchronizer,
// makeConsistentForPeriodicity).

//! Periodic node synchronizer (upstream `PeriodicNodeSynchronizer`,
//! MPI-019) and the distributed parts of periodicity (MESH-028/029 on
//! meshes distributed over several ranks).
//!
//! # The periodic node synchronizer
//!
//! A node synchronizer (`<mesh id>:periodic_synchronizer:masters`, in
//! [`crate::synchronizers::MeshSynchronizers::periodic_node`]) whose
//! **receive scheme from `p`** lists the periodic masters this rank holds
//! without owning them (parallel slaves and pure ghosts), owned by `p`, in
//! local order (upstream `std::set`), and whose **send scheme toward `q`**
//! lists the owned masters `q` asked for, in `q`'s order. Upstream builds
//! the send side with `receiveAnyNumber` (synchronous sends of global ids,
//! NBX); so does the port ([`akantu_comm::Communicator::receive_any_number`],
//! messages processed in rank order — each one only fills its sender's
//! scheme). Upstream's `update` empties the schemes but keeps their ranks
//! (`resetSchemes`), so the rank keys accumulate over successive
//! `makePeriodic` calls; the port reproduces this. The local master/slave
//! lists follow upstream's iteration order of its `unordered_multimap`
//! master → slaves for the reductions: per master, the slaves in reverse
//! insertion order ([`Mesh::periodic_slaves_upstream_order`], D-061 / UD-654,
//! the serial path's order); the copies
//! master → slaves are order-independent (a master is never a slave) and
//! use the sorted pairs.
//!
//! * `synchronizeArrayPBC` = owner → non-owner copy of the masters, then
//!   the local copy master → slaves ([`synchronize_array_pbc`]);
//! * `reduceSynchronizeArrayPBC<Op>` = local reduction slaves → masters,
//!   ghost → owner reduction and owner → ghost copy of the masters, then the
//!   local copy ([`reduce_synchronize_array_pbc`]).
//!
//! # Distributed `makePeriodic`
//!
//! See [`crate::periodic::make_periodic`]: the projected boundary boxes of
//! every rank are all-gathered (bounded payload), every rank sends the
//! owned boundary nodes inside the intersection of its box with a remote
//! box of the opposite side to that rank (point-to-point; the relation is
//! symmetric, so every rank knows whom it receives from), unknown nodes are
//! appended as pure ghosts (owner = sender, periodic bits of the sender),
//! the node synchronizer rebuilds its schemes (upstream reacts to the
//! `NewNodesEvent`), the pairs are matched locally, then the periodic
//! synchronizer is updated (NBX) and the periodic slaves are filtered out
//! of the node synchronizer (`filterScheme`, collective among neighbours).
//! Everything is computed in one [`Phases`] scope on a copy of the state
//! and committed only after every rank succeeded.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{Communicator, ReduceOp, SendMode, Synchronizer, Tag};
use akantu_core::enums::ReduceOperation;
use akantu_core::{
    Array, Error, GlobalIdx, Idx, NodeFlag, Rank, Real, Result, try_idx, try_to_usize,
};

use crate::mesh::Mesh;
use crate::synchronizers::NodeSynchronizer;

/// Tag of the periodic-scheme requests (NBX).
const UPDATE_TAG: Tag = Tag(29_310);

/// Everything the periodic synchronizer update reads, possibly ahead of the
/// mesh (the distributed `makePeriodic` computes it on its staged state).
pub(crate) struct PeriodicView<'a> {
    pub(crate) mesh: &'a Mesh,
    /// Flags of every node (existing then staged new nodes).
    pub(crate) flags: &'a [NodeFlag],
    /// Owner ranks of the staged new nodes (local id → rank).
    pub(crate) new_pranks: &'a HashMap<Idx, Rank>,
    /// Global ids of the staged new nodes (local id → global id).
    pub(crate) new_globals: &'a HashMap<Idx, GlobalIdx>,
    /// Periodic `(master, slave)` pairs.
    pub(crate) master_slaves: &'a HashMap<Idx, Vec<Idx>>,
}

impl PeriodicView<'_> {
    fn flag(&self, n: Idx) -> Result<NodeFlag> {
        self.flags
            .get(try_to_usize(n)?)
            .copied()
            .ok_or_else(|| Error::Mesh(format!("periodic synchronizer: node {n} out of range")))
    }

    fn prank(&self, n: Idx) -> Rank {
        self.new_pranks
            .get(&n)
            .copied()
            .unwrap_or_else(|| self.mesh.node_prank(n))
    }

    fn global(&self, n: Idx) -> GlobalIdx {
        self.new_globals
            .get(&n)
            .copied()
            .unwrap_or_else(|| self.mesh.node_global_id(n))
    }
}

/// Upstream `PeriodicNodeSynchronizer::update()` (`periodic_node_synchronizer.cc:41-98`)
/// as phases of `ph`: `sync`'s schemes are emptied (ranks kept), receive
/// schemes filled with the non-owned masters by owner, the owners told by
/// NBX which masters to send. One rank or a non-distributed mesh: only the
/// emptying (no communication).
pub(crate) fn update_periodic_synchronizer_view_in(
    ph: &mut Phases,
    view: &PeriodicView<'_>,
    sync: &mut NodeSynchronizer,
) -> Result<()> {
    let mesh = view.mesh;
    let comm = mesh.communicator().clone();
    let reset = |s: &mut NodeSynchronizer| {
        for v in s.communications.send.values_mut() {
            v.clear();
        }
        for v in s.communications.recv.values_mut() {
            v.clear();
        }
    };
    if !mesh.is_distributed() || comm.size() == 1 {
        ph.check("periodic:update")?;
        reset(sync);
        sync.set_entities_changed();
        return Ok(());
    }
    // local part: non-owned masters by owner (upstream std::set order)
    type Prepared = (BTreeMap<Rank, Vec<Idx>>, BTreeMap<Rank, Vec<GlobalIdx>>);
    let prepared = (|| -> Result<Prepared> {
        let mut masters: BTreeSet<Idx> = BTreeSet::new();
        for (&m, slaves) in view.master_slaves {
            if !slaves.is_empty() && !view.flag(m)?.is_local_or_master() {
                masters.insert(m);
            }
        }
        let mut recv: BTreeMap<Rank, Vec<Idx>> = BTreeMap::new();
        let mut buffers: BTreeMap<Rank, Vec<GlobalIdx>> = BTreeMap::new();
        for m in masters {
            let p = view.prank(m);
            if p < 0 || p == comm.rank() || usize::try_from(p).map_or(true, |p| p >= comm.size()) {
                return Err(Error::Mesh(format!(
                    "periodic synchronizer: the non-owned master {m} has no valid owner ({p})"
                )));
            }
            recv.entry(p).or_default().push(m);
            buffers.entry(p).or_default().push(view.global(m));
        }
        Ok((recv, buffers))
    })();
    let (recv, send) = ph.run(
        "periodic:update",
        prepared,
        |c: &Communicator, (recv, buffers)| {
            let mut sends = Vec::with_capacity(buffers.len());
            for (&p, b) in &buffers {
                sends.push(c.isend_mode(b.clone(), p, UPDATE_TAG, SendMode::Rendezvous)?);
            }
            let mut received: BTreeMap<Rank, Vec<GlobalIdx>> = BTreeMap::new();
            let mut duplicate = None;
            c.receive_any_number(sends, UPDATE_TAG, |source, msg| {
                if received.insert(source, msg).is_some() {
                    duplicate.get_or_insert(source);
                }
                Ok(())
            })?;
            if let Some(s) = duplicate {
                return Ok(Err(Error::Communication(format!(
                    "periodic synchronizer: two requests from rank {s}"
                ))));
            }
            // post-processing (rank order): global → local of the owned masters
            let send = (|| -> Result<BTreeMap<Rank, Vec<Idx>>> {
                let mut local_of: HashMap<GlobalIdx, Idx> = HashMap::new();
                for n in (0..mesh.nb_nodes()).rev() {
                    let n = try_idx(n)?;
                    local_of.insert(mesh.node_global_id(n), n);
                }
                let mut send = BTreeMap::new();
                for (p, gs) in received {
                    let mut ls = Vec::with_capacity(gs.len());
                    for g in gs {
                        ls.push(*local_of.get(&g).ok_or_else(|| {
                            Error::Mesh(format!(
                                "periodic synchronizer: master {g} requested by rank {p} is not \
                                 known locally"
                            ))
                        })?);
                    }
                    send.insert(p, ls);
                }
                Ok(send)
            })();
            Ok(send.map(|send| (recv, send)))
        },
    )?;
    reset(sync);
    for (p, v) in recv {
        sync.communications.recv.insert(p, v);
    }
    for (p, v) in send {
        sync.communications.send.insert(p, v);
    }
    sync.set_entities_changed();
    Ok(())
}

/// Upstream `Mesh::updatePeriodicSynchronizer()` on a mesh being set up:
/// the periodic node synchronizer (created if absent) is updated and the
/// periodic slaves are filtered out of the node synchronizer (collective
/// `filterScheme`). Collective on a mesh distributed over several ranks
/// (own scope; nothing changes unless every rank succeeded).
///
/// Upstream calls it only from `makePeriodic`; after distributing a mesh
/// made periodic on the root, upstream leaves non-root ranks without a
/// periodic synchronizer (`makeConsistentForPeriodicity` then dereferences
/// a null pointer, UD-254): call this first there.
pub fn update_periodic_synchronizer(mesh: &mut Mesh) -> Result<()> {
    let comm = mesh.communicator().clone();
    let mut ph = Phases::new(&comm, format!("periodic_update:{}", mesh.id()));
    let r = (|| -> Result<(NodeSynchronizer, Option<NodeSynchronizer>)> {
        let flags = mesh.nodes_flags().as_slice().to_vec();
        let empty_p = HashMap::new();
        let empty_g = HashMap::new();
        let view = PeriodicView {
            mesh,
            flags: &flags,
            new_pranks: &empty_p,
            new_globals: &empty_g,
            master_slaves: &mesh.periodic_master_slaves,
        };
        let mut sync = mesh
            .synchronizers()
            .periodic_node
            .clone()
            .unwrap_or_else(|| {
                Synchronizer::new(
                    format!("{}:periodic_synchronizer:masters", mesh.id()),
                    comm.clone(),
                )
            });
        update_periodic_synchronizer_view_in(&mut ph, &view, &mut sync)?;
        let node = filtered_node_synchronizer_in(&mut ph, mesh, &flags, None)?;
        Ok((sync, node))
    })();
    let (sync, node) = ph.settle(r)?;
    mesh.synchronizers.periodic_node = Some(sync);
    if let Some(n) = node {
        mesh.synchronizers.node = Some(n);
    }
    mirror_synchronizers_to_facets(mesh);
    Ok(())
}

/// The node synchronizer (`base`, or the mesh's) without the periodic
/// slaves (upstream `node_synchronizer->filterScheme(not isPeriodicSlave)`):
/// collective among neighbours on more than one rank, local otherwise.
pub(crate) fn filtered_node_synchronizer_in(
    ph: &mut Phases,
    mesh: &Mesh,
    flags: &[NodeFlag],
    base: Option<NodeSynchronizer>,
) -> Result<Option<NodeSynchronizer>> {
    let Some(mut ns) = base.or_else(|| mesh.synchronizers().node.clone()) else {
        return Ok(None);
    };
    let keep = |n: &Idx| {
        usize::try_from(*n)
            .ok()
            .and_then(|u| flags.get(u))
            .is_none_or(|f| !f.is_periodic_slave())
    };
    if mesh.communicator().size() > 1 && mesh.is_distributed() {
        ns.filter_scheme_in(ph, keep)?;
    } else {
        ph.check("periodic:filter")?;
        for list in ns.communications.send.values_mut() {
            list.retain(keep);
        }
        for list in ns.communications.recv.values_mut() {
            list.retain(keep);
        }
    }
    ns.set_entities_changed();
    Ok(Some(ns))
}

/// The facet mesh (if any) shares the parent's nodes, flags and periodic
/// state (upstream shares the arrays).
pub(crate) fn mirror_synchronizers_to_facets(mesh: &mut Mesh) {
    let flags = mesh.nodes_flags.clone();
    let sm = mesh.periodic_slave_master.clone();
    let ms = mesh.periodic_master_slaves.clone();
    let periodic = mesh.is_periodic;
    if let Some(f) = mesh.mesh_facets.as_deref_mut() {
        let id = f.nodes_flags.id().to_owned();
        f.nodes_flags = flags;
        f.nodes_flags.set_id(id);
        f.periodic_slave_master = sm;
        f.periodic_master_slaves = ms;
        f.is_periodic = periodic;
    }
}

/// Sorted `(master, slave)` pairs: the order of the master → slave copies
/// (order-independent).
fn pairs(mesh: &Mesh) -> Vec<(Idx, Idx)> {
    crate::periodic::periodic_pairs(mesh)
}

/// `(master, slave)` pairs in the order upstream's local reduction
/// `reduceSynchronizeArrayLocal` accumulates them (`masters_list` /
/// `slaves_list` built from the `unordered_multimap`): per master, the
/// slaves in reverse insertion order ([`Mesh::periodic_slaves_upstream_order`],
/// D-061 (2), UD-654 — the helper of the serial
/// [`crate::periodic::make_consistent_for_periodicity`]); masters are
/// independent of each other (sorted here).
fn reduction_pairs(mesh: &Mesh) -> Vec<(Idx, Idx)> {
    let mut masters: Vec<Idx> = mesh.periodic_master_slaves.keys().copied().collect();
    masters.sort_unstable();
    masters
        .iter()
        .flat_map(|&m| mesh.periodic_slaves_upstream_order(m).map(move |s| (m, s)))
        .collect()
}

fn copy_masters_to_slaves<T: Copy>(pairs: &[(Idx, Idx)], data: &mut [T], nbc: usize) -> Result<()> {
    for &(m, s) in pairs {
        let (m, s) = (try_to_usize(m)? * nbc, try_to_usize(s)? * nbc);
        if m + nbc > data.len() || s + nbc > data.len() {
            return Err(Error::Mesh("periodic pair outside the array".into()));
        }
        data.copy_within(m..m + nbc, s);
    }
    Ok(())
}

fn periodic_sync(mesh: &Mesh) -> Result<&NodeSynchronizer> {
    mesh.synchronizers().periodic_node.as_ref().ok_or_else(|| {
        Error::Mesh(format!(
            "mesh '{}' is periodic and distributed but has no periodic node synchronizer \
             (distributed after makePeriodic: call periodic_synchronizer::update_periodic_synchronizer \
             first, UD-254)",
            mesh.id()
        ))
    })
}

/// Upstream `PeriodicNodeSynchronizer::synchronizeArrayPBC`: owner →
/// non-owner copy of the masters (periodic schemes), then master → slaves
/// locally. Collective (own scope).
pub fn synchronize_array_pbc<T: akantu_comm::CommData>(
    mesh: &Mesh,
    data: &mut [T],
    nb_component: usize,
) -> Result<()> {
    let sync = periodic_sync(mesh)?;
    sync.synchronize_array(data, nb_component)?;
    copy_masters_to_slaves(&pairs(mesh), data, nb_component)
}

/// Upstream `PeriodicNodeSynchronizer::reduceSynchronizeArrayPBC<Op>`:
/// local reduction slaves → masters (per master in reverse insertion order,
/// D-061 / UD-654), ghost → owner reduction then owner → ghost copy of the
/// masters, then master → slaves locally. Collective.
pub fn reduce_synchronize_array_pbc<T: akantu_comm::CommData>(
    mesh: &Mesh,
    data: &mut [T],
    nb_component: usize,
    op: ReduceOp,
    reduce: impl Fn(T, T) -> T,
) -> Result<()> {
    let sync = periodic_sync(mesh)?;
    reduce_synchronize_array_local(mesh, data, nb_component, reduce)?;
    sync.reduce_synchronize_array(data, nb_component, op)?;
    copy_masters_to_slaves(&pairs(mesh), data, nb_component)
}

/// Upstream `PeriodicNodeSynchronizer::reduceSynchronizeArrayLocal<Op>` (the
/// first step of [`reduce_synchronize_array_pbc`]): every slave value is
/// read first, then reduced into its master, per master in reverse
/// insertion order (D-061 / UD-654). Local.
fn reduce_synchronize_array_local<T: Copy>(
    mesh: &Mesh,
    data: &mut [T],
    nb_component: usize,
    reduce: impl Fn(T, T) -> T,
) -> Result<()> {
    let order = reduction_pairs(mesh);
    // slave values before any reduction (upstream packs every slave first)
    let mut slave_values = Vec::with_capacity(order.len() * nb_component);
    for &(_, s) in &order {
        let s = try_to_usize(s)? * nb_component;
        slave_values.extend_from_slice(
            data.get(s..s + nb_component)
                .ok_or_else(|| Error::Mesh("periodic slave outside the array".into()))?,
        );
    }
    for (&(m, _), v) in order
        .iter()
        .zip(slave_values.chunks_exact(nb_component.max(1)))
    {
        let m = try_to_usize(m)? * nb_component;
        let t = data
            .get_mut(m..m + nb_component)
            .ok_or_else(|| Error::Mesh("periodic master outside the array".into()))?;
        for (a, &b) in t.iter_mut().zip(v) {
            *a = reduce(*a, b);
        }
    }
    Ok(())
}

/// Zero the values of periodic nodes that are parallel slaves or pure
/// ghosts (upstream `zero_pure_ghosts` of `makeConsistentForPeriodicity`).
fn zero_remote_periodic<T: Copy + Default>(mesh: &Mesh, data: &mut [T], nbc: usize) {
    let remote = NodeFlag::PERIODIC.bits() | NodeFlag::PURE_GHOST.bits() | NodeFlag::SLAVE.bits();
    let ghost = NodeFlag::PERIODIC.bits() | NodeFlag::PURE_GHOST.bits();
    let slave = NodeFlag::PERIODIC.bits() | NodeFlag::SLAVE.bits();
    for (n, f) in mesh.nodes_flags().as_slice().iter().enumerate() {
        let m = f.bits() & remote;
        if (m == slave || m == ghost)
            && let Some(t) = data.get_mut(n * nbc..(n + 1) * nbc)
        {
            t.fill(T::default());
        }
    }
}

/// Upstream `Mesh::makeConsistentForPeriodicity<Real>` on a mesh distributed
/// over several ranks (`mesh_periodic.cc:445-490`; collective, one scope:
/// the local checks of [`check_distributed_periodic_values`] are agreed
/// first, so a wrong array on one rank is refused on every rank instead of
/// leaving the others in the exchange; then the steps of
/// [`make_consistent_for_periodicity_real_distributed_in`]).
///
/// # Errors
/// As [`make_consistent_for_periodicity_real_distributed_in`]; a local
/// failure on another rank ("failed on another rank").
pub fn make_consistent_for_periodicity_real_distributed(
    mesh: &Mesh,
    array: &mut Array<Real>,
    operation: ReduceOperation,
) -> Result<()> {
    let nbc = array.nb_component();
    let mut ph = Phases::new(mesh.communicator(), format!("periodic:{}", mesh.id()));
    let check = check_distributed_periodic_values(mesh, array.as_slice().len(), nbc);
    let r = ph
        .agree("periodic:prepare", check, Severity::Recoverable)
        .and_then(|()| {
            make_consistent_for_periodicity_real_distributed_in(
                mesh,
                &mut ph,
                array.as_mut_slice(),
                nbc,
                operation,
            )
        });
    ph.settle(r)
}

/// The local preconditions of the distributed `makeConsistentForPeriodicity`
/// on `len` values of `nb_component` components: one tuple per node, and
/// the periodic node synchronizer (UD-254). Local; the own-scope entry
/// points agree it before their first exchange.
///
/// # Errors
/// [`Error::DimensionMismatch`], [`Error::Mesh`] (no periodic synchronizer).
pub fn check_distributed_periodic_values(
    mesh: &Mesh,
    len: usize,
    nb_component: usize,
) -> Result<()> {
    check_nodal_values(mesh, len, nb_component)?;
    periodic_sync(mesh).map(|_| ())
}

/// [`make_consistent_for_periodicity_real_distributed`] on the values of a
/// nodal array (`nb_nodes` tuples of `nb_component`) as phases of an
/// enclosing collective operation, e.g. a model's residual or lumped-mass
/// assembly inside the DOF manager's scope (`DofManager::phases`), where an
/// independent scope is refused (ARCHITECTURE). The same steps and
/// values as the own-scope function.
///
/// # Errors
/// [`Error::DimensionMismatch`] for an array that is not nodal; a missing
/// periodic synchronizer (UD-254); communication failures (recorded in
/// `phases`).
pub fn make_consistent_for_periodicity_real_distributed_in(
    mesh: &Mesh,
    phases: &mut Phases,
    data: &mut [Real],
    nb_component: usize,
    operation: ReduceOperation,
) -> Result<()> {
    check_nodal_values(mesh, data.len(), nb_component)?;
    let sync = periodic_sync(mesh)?;
    match operation {
        ReduceOperation::Id => sync.synchronize_array_in(phases, data, nb_component)?,
        ReduceOperation::Sum | ReduceOperation::Or => {
            sum_contributions(mesh, data, nb_component)?;
            sync.reduce_synchronize_array_in(phases, data, nb_component, ReduceOp::Sum)?;
        }
    }
    copy_masters_to_slaves(&pairs(mesh), data, nb_component)
}

/// [`make_consistent_for_periodicity_bool_distributed`] on the values of a
/// nodal array as phases of an enclosing collective operation.
///
/// # Errors
/// As [`make_consistent_for_periodicity_real_distributed_in`].
pub fn make_consistent_for_periodicity_bool_distributed_in(
    mesh: &Mesh,
    phases: &mut Phases,
    data: &mut [bool],
    nb_component: usize,
    operation: ReduceOperation,
) -> Result<()> {
    check_nodal_values(mesh, data.len(), nb_component)?;
    let sync = periodic_sync(mesh)?;
    let mut bytes: Vec<u8> = data.iter().map(|&b| u8::from(b)).collect();
    match operation {
        ReduceOperation::Id => sync.synchronize_array_in(phases, &mut bytes, nb_component)?,
        ReduceOperation::Sum | ReduceOperation::Or => {
            zero_remote_periodic(mesh, &mut bytes, nb_component);
            reduce_synchronize_array_local(mesh, &mut bytes, nb_component, |a, b| {
                u8::from(a != 0 || b != 0)
            })?;
            sync.reduce_synchronize_array_in(
                phases,
                &mut bytes,
                nb_component,
                ReduceOp::LogicalOr,
            )?;
        }
    }
    copy_masters_to_slaves(&pairs(mesh), &mut bytes, nb_component)?;
    for (d, b) in data.iter_mut().zip(bytes) {
        *d = b != 0;
    }
    Ok(())
}

/// `len` values hold one tuple of `nb_component` per node.
fn check_nodal_values(mesh: &Mesh, len: usize, nb_component: usize) -> Result<()> {
    if nb_component == 0 || len != mesh.nb_nodes() * nb_component {
        return Err(Error::DimensionMismatch {
            context: "makeConsistentForPeriodicity (values = nb_nodes × nb_component)",
            expected: mesh.nb_nodes() * nb_component,
            got: len,
        });
    }
    Ok(())
}

/// The local part of `makeConsistentForPeriodicity(_sum)`: zero the
/// periodic parallel slaves and pure ghosts, then reduce every local slave
/// into its master (the first steps of
/// [`make_consistent_for_periodicity_real_distributed`], same code).
fn sum_contributions(mesh: &Mesh, data: &mut [Real], nbc: usize) -> Result<()> {
    zero_remote_periodic(mesh, data, nbc);
    reduce_synchronize_array_local(mesh, data, nbc, |a, b| a + b)
}

/// The per-rank contributions of `makeConsistentForPeriodicity(array,
/// _sum)` (D-039 amendment): `array` after upstream's
/// `zero_pure_ghosts` and the local reduction slaves → masters
/// (`reduceSynchronizeArrayLocal`, D-061 / UD-654), i.e. before any
/// communication. At a periodic master copy this rank does not own, the
/// value is what it sends to the owner in the ghost → owner reduction; at
/// an owned master it is the value the remote contributions are added to
/// (in rank order here, D-039; in arrival order upstream). The distributed
/// `_sum` runs exactly this code first. Local (no communication); no-op on
/// a non-periodic mesh.
///
/// # Errors
/// [`Error::DimensionMismatch`] if `array` does not hold one tuple per
/// node; [`Error::Mesh`] if a periodic pair lies outside the array.
pub fn periodic_sum_contributions_real(mesh: &Mesh, array: &mut Array<Real>) -> Result<()> {
    if !mesh.is_periodic() {
        return Ok(());
    }
    if array.len() != mesh.nb_nodes() {
        return Err(Error::DimensionMismatch {
            context: "periodic_sum_contributions_real (array length = nb_nodes)",
            expected: mesh.nb_nodes(),
            got: array.len(),
        });
    }
    let nbc = array.nb_component();
    sum_contributions(mesh, array.as_mut_slice(), nbc)
}

/// Upstream `Mesh::makeConsistentForPeriodicity<bool>` on a mesh distributed
/// over several ranks (`_sum` and `_or` are logical or; collective, one
/// scope with the local checks agreed first, as
/// [`make_consistent_for_periodicity_real_distributed`]; then the steps of
/// [`make_consistent_for_periodicity_bool_distributed_in`]).
///
/// # Errors
/// As [`make_consistent_for_periodicity_bool_distributed_in`]; a local
/// failure on another rank ("failed on another rank").
pub fn make_consistent_for_periodicity_bool_distributed(
    mesh: &Mesh,
    array: &mut Array<bool>,
    operation: ReduceOperation,
) -> Result<()> {
    let nbc = array.nb_component();
    let mut ph = Phases::new(mesh.communicator(), format!("periodic:{}", mesh.id()));
    let check = check_distributed_periodic_values(mesh, array.as_slice().len(), nbc);
    let r = ph
        .agree("periodic:prepare", check, Severity::Recoverable)
        .and_then(|()| {
            make_consistent_for_periodicity_bool_distributed_in(
                mesh,
                &mut ph,
                array.as_mut_slice(),
                nbc,
                operation,
            )
        });
    ph.settle(r)
}

/// Agreement helper for the staged distributed `makePeriodic`.
pub(crate) fn agreed<T>(ph: &mut Phases, name: &str, r: Result<T>) -> Result<T> {
    ph.agreed(name, r, Severity::Recoverable)
}
