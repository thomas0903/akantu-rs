// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/ntn_contact/ntn_base_contact.cc (initParallel, the synch_registry
// calls of computeContactPressure), ntn_contact.cc (updateNormals),
// ntn_base_friction.cc (updateSlip), ntn_base_contact_inline_impl.hh
// (getNbData / packData / unpackData of _cf_nodal and _cf_incr);
// src/synchronizer/synchronizer_impl_tmpl.hh (filterScheme).

//! The split-node contact on distributed meshes (MPI-051, MPI-052,
//! decision D-083).
//!
//! **`init_parallel`** (upstream's public `NTNBaseContact::initParallel()`,
//! which no upstream program calls): a copy of the mesh's element
//! synchronizer, filtered collectively (upstream `filterScheme`: each rank
//! keeps the received elements with a node in its contact — slave or
//! master of a pair, upstream `getNodeIndex(node) >= 0` — and tells their
//! owners which positions to keep).
//!
//! **Exchanges** over that synchronizer, owner → ghost, every node of every
//! element of the schemes in connectivity order (upstream
//! `packNodalDataHelper`, one array after the other): `_cf_nodal` carries
//! the displacement, the current position and the velocity (upstream
//! `computeContactPressure` and `NTNContact::updateNormals` start with it),
//! `_cf_incr` the displacement increment (upstream `updateSlip`). The
//! receiver writes the solid model's arrays (upstream: the model's
//! displacement, velocity and increment, and the model's current position
//! through a `const_cast`); the releases are unchanged, as upstream. The
//! current position is refreshed before the exchange on both sides
//! (upstream refreshes it inside pack / unpack, after the displacement was
//! unpacked: the same values, since the nodes whose displacement the
//! exchange changes are the nodes whose current position it overwrites).
//!
//! **Without `init_parallel`** on a distributed mesh upstream's exchanges
//! do nothing (no synchronizer is registered) and the contact reads stale
//! ghost data (UD-904, category A): the port refuses the exchanges, hence
//! the solver callback's step and [`NtnBaseContact::synchronize_nodal`]
//! (`Error::Unsupported`, agreed on every rank by the step).
//!
//! **Normals**: upstream's `NTNContact::updateNormals` starts
//! with `synchronize(_cf_nodal)`; [`NtnBaseContact::update_normals_in`] /
//! [`NtnBaseContact::update_normals_of`] do the same, and the read-only
//! [`NtnBaseContact::update_normals`] is refused after `init_parallel` on a
//! mesh distributed over more than one rank (it cannot exchange). Contact
//! nodes added after `init_parallel` there are refused as well, by every
//! addition (`add_surface_pair`, `add_node_pairs`, the low-level
//! `add_split_node` and the rigid-flat `add_surface` / `add_nodes`):
//! they would lie outside the filtered synchronizer, a node
//! pair would escape the pairing check below and update the normals
//! without the exchange.
//!
//! **Per-rank pairing** (D-097 (2), UD-1105): `NTNContact::addSurfacePair`
//! pairs the nodes each rank holds, as upstream, with no cross-rank
//! pairing. On a partition that separates the two sides of an interface
//! upstream silently drops pairs, and a rank that sees one slave node pairs
//! it with an arbitrary master (its tolerance `0.1 · sqrt(min distance)`
//! is then about 1e153). `init_parallel` therefore compares, for every
//! surface pair, the pairs each rank formed with the pairing of the whole
//! mesh (the group nodes of every rank gathered, paired with upstream's
//! rule in global-id order) and refuses, with the same `Error::Unsupported`
//! on every rank and nothing installed (recoverable), a partition on which
//! (a) a slave node owned by a rank lacks on that rank a pair of the global
//! pairing, (b) a rank that sees exactly one slave node of the surface (and
//! a master node) would pair it, or (c) a rank forms a pair the global
//! pairing does not.
//!
//! **Cost of the check**: every rank gathers every
//! rank's record (per group node: its global id, flags and coordinates; a
//! node that is a ghost on several ranks is counted on each) and pairs the
//! whole interface with upstream's quadratic rule, `O(N_s² + N_s·N_m)`
//! distance evaluations for `N_s` slave and `N_m` master nodes, once per
//! surface pair at `init_parallel`. That is negligible for the interfaces
//! of this package's uses (tens of nodes) and about 5·10⁹ evaluations on
//! every rank for an interface of 10⁵ slave nodes.

use std::collections::{BTreeMap, BTreeSet};

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::phase::Phases;
use akantu_comm::{DataAccessor, Synchronizer};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Element, Error, Real, Result};
use akantu_mesh::Mesh;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::{DOF_ID, SolidMechanicsPhysics};
use akantu_solver::DofManager;

use crate::contact::NtnBaseContact;

/// Bytes of one `Real`.
const R: usize = std::mem::size_of::<Real>();

/// Nodal arrays at the nodes of the scheme elements (upstream
/// `packNodalDataHelper` / `unpackNodalDataHelper`, one array after the
/// other).
struct NodalAccessor<'a, 'b> {
    mesh: &'a Mesh,
    dim: usize,
    arrays: Vec<&'b mut [Real]>,
}

impl NodalAccessor<'_, '_> {
    fn nodes(&self, e: Element) -> Result<Vec<usize>> {
        self.mesh
            .element_connectivity(e)?
            .iter()
            .map(|&n| {
                usize::try_from(n).map_err(|_| Error::InvalidIndex {
                    what: "node",
                    value: i64::from(n),
                })
            })
            .collect()
    }
}

impl DataAccessor<Element> for NodalAccessor<'_, '_> {
    fn nb_data(&self, elements: &[Element], _tag: SynchronizationTag) -> usize {
        let mut nb_nodes = 0usize;
        for e in elements {
            nb_nodes = nb_nodes.saturating_add(e.ty().nb_nodes_per_element());
        }
        nb_nodes
            .checked_mul(self.dim)
            .and_then(|v| v.checked_mul(R))
            .and_then(|v| v.checked_mul(self.arrays.len()))
            .unwrap_or(usize::MAX)
    }

    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        elements: &[Element],
        _tag: SynchronizationTag,
    ) -> Result<()> {
        let d = self.dim;
        for a in &self.arrays {
            for &e in elements {
                for n in self.nodes(e)? {
                    let v = a.get(n * d..(n + 1) * d).ok_or_else(|| {
                        Error::invalid("split-node contact exchange: nodal array too short")
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
        let d = self.dim;
        let nodes: Vec<Vec<usize>> = elements
            .iter()
            .map(|&e| self.nodes(e))
            .collect::<Result<_>>()?;
        for a in &mut self.arrays {
            for ns in &nodes {
                for &n in ns {
                    let v = a.get_mut(n * d..(n + 1) * d).ok_or_else(|| {
                        Error::invalid("split-node contact exchange: nodal array too short")
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

impl NtnBaseContact {
    /// Upstream `initParallel()`: the filtered copy of the mesh's element
    /// synchronizer (module documentation). Collective on a distributed
    /// mesh (the pairing check, then one `filterScheme`); a mesh without an
    /// element synchronizer (one rank) needs nothing. Call it after every
    /// contact node was added (upstream filters with the nodes known then;
    /// later node pairs are refused on a mesh distributed over more than
    /// one rank).
    ///
    /// # Errors
    /// On a mesh distributed over more than one rank, a partition on which
    /// the per-rank pairing of a surface pair differs from the pairing of
    /// the whole mesh (module documentation; UD-1105, category A:
    /// `Error::Unsupported` on every rank, nothing installed); communication
    /// errors (agreed on every rank).
    pub fn init_parallel(&mut self, mesh: &Mesh) -> Result<()> {
        let Some(es) = mesh.synchronizers().element.as_ref() else {
            return Ok(());
        };
        let exchanging = mesh.communicator().size() > 1;
        if exchanging {
            self.check_pairing(mesh)?;
        }
        let mut sync = Synchronizer::new(
            format!("{}:contact_synchronizer", self.id()),
            es.communicator.clone(),
        );
        sync.copy_schemes(es);
        let contact = &*self;
        let mut bad = None;
        sync.filter_scheme(|e| match mesh.element_connectivity(*e) {
            Ok(conn) => conn.iter().any(|&n| contact.node_index(n).is_some()),
            Err(err) => {
                bad.get_or_insert(err);
                false
            }
        })?;
        if let Some(err) = bad {
            return Err(err);
        }
        self.set_synchronizer(Some(sync), exchanging);
        Ok(())
    }

    /// The pairing check of [`NtnBaseContact::init_parallel`] (module
    /// documentation): every rank gathers every rank's record and computes
    /// the same verdict, agreed once more on every rank.
    fn check_pairing(&self, mesh: &Mesh) -> Result<()> {
        let comm = mesh.communicator().clone();
        let mut ph = Phases::new(&comm, format!("{}:init_parallel", self.id()));
        let prepared = pairing_record(self, mesh);
        let r = ph.run("ntn:pairing_check", prepared, |c, rec| {
            let all = c.all_gather_v(&rec)?;
            Ok(pairing_verdict(&all))
        });
        ph.settle(r)
    }

    /// The filtered synchronizer of [`NtnBaseContact::init_parallel`].
    #[must_use]
    pub fn synchronizer(&self) -> Option<&Synchronizer<Element>> {
        self.synchronizer_ref()
    }

    /// Refuse when the contact must exchange before reading ghost data (a
    /// mesh distributed over more than one rank) and has no synchronizer:
    /// upstream's default path reads stale ghost data (UD-904, category A).
    pub(crate) fn refuse_without_init_parallel(&self, mesh: &Mesh) -> Result<()> {
        if mesh.communicator().size() > 1
            && mesh.synchronizers().element.is_some()
            && self.synchronizer_ref().is_none()
        {
            return Err(Error::Unsupported(
                "split-node contact on a distributed mesh without init_parallel(): upstream \
                 exchanges nothing then and the contact reads stale ghost data (UD-904, \
                 category A); call NtnBaseContact::init_parallel after the contact nodes were \
                 added"
                    .into(),
            ));
        }
        Ok(())
    }

    /// The `_cf_nodal` (displacement, current position, velocity) or
    /// `_cf_incr` (increment) exchange inside a running collective
    /// operation of `dofs` (a solver callback). One rank: nothing.
    ///
    /// # Errors
    /// A distributed mesh without [`NtnBaseContact::init_parallel`]
    /// (UD-904), another tag, missing arrays, communication errors.
    pub fn synchronize_nodal_in(
        &self,
        tag: SynchronizationTag,
        physics: &mut SolidMechanicsPhysics,
        dofs: &mut DofManager,
    ) -> Result<()> {
        let mesh_h = physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.refuse_without_init_parallel(&mesh)?;
        let Some(sync) = self.synchronizer_ref() else {
            return Ok(());
        };
        let dim = physics.base.spatial_dimension;
        if tag == SynchronizationTag::CfNodal && physics.cached_current_position().is_ok() {
            physics.refresh_current_position(dofs)?;
        }
        let (mut guard, family) = dofs.phases_with_family(DOF_ID)?;
        let family = family.ok_or_else(|| {
            Error::invalid("split-node contact: the model has no displacement (init_full first)")
        })?;
        match tag {
            SynchronizationTag::CfNodal => {
                let mut v_arr = family
                    .derivatives
                    .get_mut(&1)
                    .ok_or_else(|| {
                        Error::invalid(
                            "split-node contact: _cf_nodal needs the velocity (dynamic analysis)",
                        )
                    })?
                    .write()?;
                let mut u = family.dofs.write()?;
                let mut cur = physics.current_position_mut()?;
                let mut acc = NodalAccessor {
                    mesh: &mesh,
                    dim,
                    arrays: vec![u.as_mut_slice(), cur.as_mut_slice(), v_arr.as_mut_slice()],
                };
                sync.synchronize_in(&mut guard, &mut acc, tag)
            }
            SynchronizationTag::CfIncr => {
                let mut inc = family
                    .increment
                    .as_mut()
                    .ok_or_else(|| {
                        Error::invalid(
                            "split-node contact: _cf_incr needs the displacement increment",
                        )
                    })?
                    .write()?;
                let mut acc = NodalAccessor {
                    mesh: &mesh,
                    dim,
                    arrays: vec![inc.as_mut_slice()],
                };
                sync.synchronize_in(&mut guard, &mut acc, tag)
            }
            other => Err(Error::invalid(format!(
                "split-node contact: no exchange for the tag {other}"
            ))),
        }
    }

    /// [`NtnBaseContact::synchronize_nodal_in`] as its own collective
    /// operation on `model` (upstream `getSynchronizerRegistry().synchronize
    /// (tag)` called by a user program).
    ///
    /// # Errors
    /// As [`NtnBaseContact::synchronize_nodal_in`].
    pub fn synchronize_nodal(
        &self,
        tag: SynchronizationTag,
        model: &mut SolidMechanicsModel,
    ) -> Result<()> {
        let physics = &mut model.physics;
        model
            .solver
            .dof_manager
            .collective("ntn:synchronize", |dm| {
                self.synchronize_nodal_in(tag, physics, dm)
            })
    }
}

/// Upstream's `allReduce(_sum)` of a count over the mesh's communicator
/// (`getNbNodesInContact`, `getNbStickingNodes`): the local value on a
/// serial mesh. `local` is the rank's outcome: a rank-local error is agreed
/// before the reduction, so every rank returns an error.
pub(crate) fn sum_count(mesh: &Mesh, local: Result<usize>, what: &str) -> Result<usize> {
    if mesh.synchronizers().element.is_none() {
        return local;
    }
    let comm = mesh.communicator().clone();
    let mut ph = Phases::new(&comm, format!("ntn:{what}"));
    let prepared =
        local.and_then(|v| u64::try_from(v).map_err(|_| Error::invalid("count overflows u64")));
    let r = ph
        .run_collective(what, prepared, |c, v| {
            let mut a = [v];
            c.all_reduce(&mut a, akantu_comm::ReduceOp::Sum)?;
            Ok(a[0])
        })
        .and_then(|v| usize::try_from(v).map_err(|_| Error::invalid("count overflows usize")));
    ph.settle(r)
}

/// Upstream's `allReduce(_sum)` of reals (`getExternalWork`,
/// `integrateResidual`): rank-ordered on the port's communicators (D-039).
/// `local` is the outcome of computing `values` on this rank: a rank-local
/// error is agreed before the reduction; on a serial mesh it
/// is returned as is.
pub(crate) fn sum_reals(
    mesh: &Mesh,
    local: Result<()>,
    values: &mut [Real],
    what: &str,
) -> Result<()> {
    if mesh.synchronizers().element.is_none() {
        return local;
    }
    let comm = mesh.communicator().clone();
    let mut ph = Phases::new(&comm, format!("ntn:{what}"));
    let r = ph.run_collective(what, local, |c, ()| {
        c.all_reduce(values, akantu_comm::ReduceOp::Sum)
    });
    ph.settle(r)
}

// ---- pairing check of init_parallel (UD-1105) ----------------

/// One rank's view of a surface pair (record of [`pairing_record`]).
#[derive(Debug, Default)]
struct PairingView {
    digest: i64,
    surface_normal_dir: usize,
    keep_searching: bool,
    names: (String, String),
    /// Slave nodes the rank sees (its group node list, local and ghost).
    nb_slaves_seen: usize,
    /// Master nodes the rank sees.
    nb_masters_seen: usize,
    /// `(global id, owned by the rank, periodic slave, coordinates)` of the
    /// slave nodes the rank sees.
    slaves: Vec<(i64, bool, bool, Vec<Real>)>,
    /// `(global id, periodic slave, coordinates)` of its master nodes.
    masters: Vec<(i64, bool, Vec<Real>)>,
    /// The pairs the rank forms, by global ids.
    pairs: Vec<(i64, i64)>,
}

/// Digest of a surface pair's arguments (the ranks must have added the
/// same surface pairs).
fn pairing_digest(p: &crate::contact::SurfacePairing) -> i64 {
    let mut h = akantu_comm::phase::digest_bytes(0, p.slave.as_bytes());
    h = akantu_comm::phase::digest_bytes(h, &[0]);
    h = akantu_comm::phase::digest_bytes(h, p.master.as_bytes());
    h = akantu_comm::phase::digest_bytes(h, &[0]);
    h = akantu_comm::phase::digest_bytes(h, &(p.surface_normal_dir as u64).to_le_bytes());
    h = akantu_comm::phase::digest_bytes(h, &[u8::from(p.keep_searching)]);
    // the bits of the digest, reinterpreted (not a quantity)
    h as i64
}

fn len_i64(n: usize) -> Result<i64> {
    i64::try_from(n).map_err(|_| Error::invalid("pairing record: length overflows i64"))
}

/// This rank's view of every surface pair, encoded for the pairing check
/// ([`encode_views`]).
fn pairing_record(contact: &NtnBaseContact, mesh: &Mesh) -> Result<Vec<i64>> {
    let dim = mesh.spatial_dimension();
    let coords = mesh.nodes();
    let gid = |n: akantu_core::Idx| -> i64 {
        if mesh.is_distributed() {
            mesh.node_global_id(n)
        } else {
            i64::from(n)
        }
    };
    let node = |n: akantu_core::Idx| -> Result<usize> {
        usize::try_from(n).map_err(|_| Error::InvalidIndex {
            what: "node",
            value: i64::from(n),
        })
    };
    let mut views = Vec::new();
    for p in contact.surface_pairings() {
        let groups = mesh.groups();
        let slaves = groups.element_group(&p.slave)?.nodes().as_slice().to_vec();
        let masters = groups.element_group(&p.master)?.nodes().as_slice().to_vec();
        let mut v = PairingView {
            digest: pairing_digest(p),
            surface_normal_dir: p.surface_normal_dir,
            keep_searching: p.keep_searching,
            names: (p.slave.clone(), p.master.clone()),
            nb_slaves_seen: slaves.len(),
            nb_masters_seen: masters.len(),
            ..PairingView::default()
        };
        for &s in &slaves {
            v.slaves.push((
                gid(s),
                mesh.is_local_or_master_node(s)?,
                mesh.is_periodic_slave(s)?,
                coords.get_tuple(node(s)?)?.to_vec(),
            ));
        }
        for &m in &masters {
            v.masters.push((
                gid(m),
                mesh.is_periodic_slave(m)?,
                coords.get_tuple(node(m)?)?.to_vec(),
            ));
        }
        v.pairs = NtnBaseContact::surface_pairs(mesh, p)?
            .into_iter()
            .map(|(s, m)| (gid(s), gid(m)))
            .collect();
        views.push(v);
    }
    encode_views(dim, &views)
}

/// The record of one rank: `[dim, K]`, then per surface pair `[digest,
/// dir, keep, nb slaves seen, nb masters seen, nS, (gid, owned, periodic,
/// coordinate bits…)…, nM, (gid, periodic, coordinate bits…)…, nP, (gid
/// slave, gid master)…, the two surface names (length, bytes)]`.
fn encode_views(dim: usize, views: &[PairingView]) -> Result<Vec<i64>> {
    let mut rec = vec![len_i64(dim)?, len_i64(views.len())?];
    for v in views {
        rec.push(v.digest);
        rec.push(len_i64(v.surface_normal_dir)?);
        rec.push(i64::from(v.keep_searching));
        rec.push(len_i64(v.nb_slaves_seen)?);
        rec.push(len_i64(v.nb_masters_seen)?);
        rec.push(len_i64(v.slaves.len())?);
        for (g, owned, periodic, x) in &v.slaves {
            if x.len() != dim {
                return Err(Error::invalid(
                    "pairing record: coordinates of another dimension",
                ));
            }
            rec.extend([*g, i64::from(*owned), i64::from(*periodic)]);
            // the bits of the coordinates, reinterpreted (exact transport)
            rec.extend(x.iter().map(|c| c.to_bits() as i64));
        }
        rec.push(len_i64(v.masters.len())?);
        for (g, periodic, x) in &v.masters {
            if x.len() != dim {
                return Err(Error::invalid(
                    "pairing record: coordinates of another dimension",
                ));
            }
            rec.extend([*g, i64::from(*periodic)]);
            rec.extend(x.iter().map(|c| c.to_bits() as i64));
        }
        rec.push(len_i64(v.pairs.len())?);
        for &(s, m) in &v.pairs {
            rec.extend([s, m]);
        }
        for name in [&v.names.0, &v.names.1] {
            rec.push(len_i64(name.len())?);
            rec.extend(name.bytes().map(i64::from));
        }
    }
    Ok(rec)
}

/// Reader of a gathered record.
struct Cursor<'a> {
    v: &'a [i64],
    at: usize,
}

impl Cursor<'_> {
    fn next(&mut self) -> Result<i64> {
        let x =
            self.v.get(self.at).copied().ok_or_else(|| {
                Error::Communication("split-node pairing record truncated".into())
            })?;
        self.at += 1;
        Ok(x)
    }

    fn len(&mut self) -> Result<usize> {
        usize::try_from(self.next()?)
            .map_err(|_| Error::Communication("split-node pairing record: negative length".into()))
    }

    fn reals(&mut self, n: usize) -> Result<Vec<Real>> {
        (0..n)
            .map(|_| self.next().map(|b| Real::from_bits(b as u64)))
            .collect()
    }

    fn string(&mut self) -> Result<String> {
        let n = self.len()?;
        let bytes: Vec<u8> = (0..n)
            .map(|_| {
                self.next().and_then(|b| {
                    u8::try_from(b).map_err(|_| {
                        Error::Communication("split-node pairing record: bad name byte".into())
                    })
                })
            })
            .collect::<Result<_>>()?;
        String::from_utf8(bytes)
            .map_err(|_| Error::Communication("split-node pairing record: name not UTF-8".into()))
    }
}

/// Parse one rank's record: the dimension and its views.
fn parse_record(v: &[i64]) -> Result<(usize, Vec<PairingView>)> {
    let mut c = Cursor { v, at: 0 };
    let dim = c.len()?;
    let k = c.len()?;
    let mut views = Vec::with_capacity(k.min(64));
    for _ in 0..k {
        let mut p = PairingView {
            digest: c.next()?,
            surface_normal_dir: c.len()?,
            keep_searching: c.next()? != 0,
            nb_slaves_seen: c.len()?,
            nb_masters_seen: c.len()?,
            ..PairingView::default()
        };
        for _ in 0..c.len()? {
            let g = c.next()?;
            let owned = c.next()? != 0;
            let periodic = c.next()? != 0;
            p.slaves.push((g, owned, periodic, c.reals(dim)?));
        }
        for _ in 0..c.len()? {
            let g = c.next()?;
            let periodic = c.next()? != 0;
            p.masters.push((g, periodic, c.reals(dim)?));
        }
        for _ in 0..c.len()? {
            let s = c.next()?;
            let m = c.next()?;
            p.pairs.push((s, m));
        }
        p.names = (c.string()?, c.string()?);
        views.push(p);
    }
    if c.at != v.len() {
        return Err(Error::Communication(
            "split-node pairing record: trailing values".into(),
        ));
    }
    Ok((dim, views))
}

/// Upstream `pairInterfaceNodes`' rule on the nodes of the whole mesh, in
/// global-id order (`slaves` / `masters`: global id → (periodic slave,
/// coordinates)), without the pairs holding a periodic slave node: the
/// same expressions as [`NtnBaseContact::pair_interface_nodes`].
fn global_pairs(
    slaves: &BTreeMap<i64, (bool, Vec<Real>)>,
    masters: &BTreeMap<i64, (bool, Vec<Real>)>,
    dim: usize,
    surface_normal_dir: usize,
    keep_searching: bool,
) -> BTreeSet<(i64, i64)> {
    let offset: Vec<usize> = (0..dim).filter(|&i| i != surface_normal_dir).collect();
    let pd = offset.len();
    let project = |x: &[Real]| -> Vec<Real> { offset.iter().map(|&o| x[o]).collect() };
    let ps: Vec<(i64, bool, Vec<Real>)> = slaves
        .iter()
        .map(|(&g, (per, x))| (g, *per, project(x)))
        .collect();
    let pm: Vec<(i64, bool, Vec<Real>)> = masters
        .iter()
        .map(|(&g, (per, x))| (g, *per, project(x)))
        .collect();
    let mut min_dist = Real::MAX;
    for i in 0..ps.len() {
        for j in i + 1..ps.len() {
            let mut dist = 0.0;
            for d in 0..pd {
                dist += (ps[i].2[d] - ps[j].2[d]) * (ps[i].2[d] - ps[j].2[d]);
            }
            if dist < min_dist {
                min_dist = dist;
            }
        }
    }
    let local_tol = 0.1 * min_dist.sqrt();
    let mut out = BTreeSet::new();
    for s in &ps {
        for m in &pm {
            let mut dist = 0.0;
            for d in 0..pd {
                dist += (s.2[d] - m.2[d]) * (s.2[d] - m.2[d]);
            }
            if dist.sqrt() < local_tol {
                if !s.1 && !m.1 {
                    out.insert((s.0, m.0));
                }
                if !keep_searching {
                    break;
                }
            }
        }
    }
    out
}

/// The verdict of the pairing check from every rank's record (rank
/// order): the same on every rank.
fn pairing_verdict(all: &[Vec<i64>]) -> Result<()> {
    let ranks: Vec<(usize, Vec<PairingView>)> =
        all.iter().map(|v| parse_record(v)).collect::<Result<_>>()?;
    let Some((dim, first)) = ranks.first() else {
        return Ok(());
    };
    for (r, (d, views)) in ranks.iter().enumerate() {
        let same = *d == *dim
            && views.len() == first.len()
            && views.iter().zip(first).all(|(a, b)| a.digest == b.digest);
        if !same {
            return Err(Error::invalid(format!(
                "split-node contact init_parallel: rank {r} added other surface pairs than rank \
                 0 (addSurfacePair must be called with the same arguments on every rank)"
            )));
        }
    }
    let mut problems: Vec<String> = Vec::new();
    let mut count = 0usize;
    for (k, view0) in first.iter().enumerate() {
        let mut slaves: BTreeMap<i64, (bool, Vec<Real>)> = BTreeMap::new();
        let mut masters: BTreeMap<i64, (bool, Vec<Real>)> = BTreeMap::new();
        for (_, views) in &ranks {
            for (g, _, per, x) in &views[k].slaves {
                slaves.entry(*g).or_insert_with(|| (*per, x.clone()));
            }
            for (g, per, x) in &views[k].masters {
                masters.entry(*g).or_insert_with(|| (*per, x.clone()));
            }
        }
        let global = global_pairs(
            &slaves,
            &masters,
            *dim,
            view0.surface_normal_dir,
            view0.keep_searching,
        );
        let mut by_slave: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for &(s, m) in &global {
            by_slave.entry(s).or_default().push(m);
        }
        let surface = format!("{} / {}", view0.names.0, view0.names.1);
        let mut note = |problems: &mut Vec<String>, what: String| {
            count += 1;
            if problems.len() < 3 {
                problems.push(format!("surfaces {surface}: {what}"));
            }
        };
        for (r, (_, views)) in ranks.iter().enumerate() {
            let v = &views[k];
            let local: BTreeSet<(i64, i64)> = v.pairs.iter().copied().collect();
            if v.nb_slaves_seen == 1 && v.nb_masters_seen >= 1 {
                let g = v.slaves.first().map_or(-1, |s| s.0);
                note(
                    &mut problems,
                    format!(
                        "rank {r} sees one slave node (global id {g}) and pairs it with an \
                         arbitrary master node"
                    ),
                );
            }
            for &(s, m) in &local {
                if !global.contains(&(s, m)) {
                    note(
                        &mut problems,
                        format!(
                            "rank {r} pairs slave node {s} with master node {m}, which the \
                             pairing of the whole mesh does not pair"
                        ),
                    );
                }
            }
            for (s, owned, _, _) in &v.slaves {
                if !*owned {
                    continue;
                }
                for &m in by_slave.get(s).map_or(&[][..], Vec::as_slice) {
                    if !local.contains(&(*s, m)) {
                        note(
                            &mut problems,
                            format!(
                                "slave node {s}, owned by rank {r}, is not paired there with \
                                 master node {m} (its pair in the whole mesh)"
                            ),
                        );
                    }
                }
            }
        }
    }
    if count == 0 {
        return Ok(());
    }
    Err(Error::Unsupported(format!(
        "split-node contact on a distributed mesh: on this partition the per-rank pairing \
         differs from the pairing of the whole mesh ({count} case(s); {}); upstream silently \
         drops such pairs or pairs a slave node with an arbitrary master (UD-1105, category \
         A): partition the mesh so that both sides of every interface node pair lie on the \
         same ranks (e.g. strips across the interface)",
        problems.join("; ")
    )))
}

#[cfg(test)]
mod tests {
    //! The verdict of the pairing check on synthetic records (cases the
    //! integration tests cannot easily build from a partition: a rank that
    //! sees one slave node, a pair formed with a larger rank tolerance).
    use super::*;

    /// A view of the surfaces "s" / "m" (dir 1, coordinates `(x, 0)`).
    fn view(
        slaves: &[(i64, bool, Real)],
        masters: &[(i64, Real)],
        pairs: &[(i64, i64)],
    ) -> PairingView {
        PairingView {
            digest: 7,
            surface_normal_dir: 1,
            keep_searching: false,
            names: ("s".into(), "m".into()),
            nb_slaves_seen: slaves.len(),
            nb_masters_seen: masters.len(),
            slaves: slaves
                .iter()
                .map(|&(g, o, x)| (g, o, false, vec![x, 0.0]))
                .collect(),
            masters: masters
                .iter()
                .map(|&(g, x)| (g, false, vec![x, 0.0]))
                .collect(),
            pairs: pairs.to_vec(),
        }
    }

    fn verdict(views: Vec<PairingView>) -> Result<()> {
        let all: Vec<Vec<i64>> = views
            .into_iter()
            .map(|v| encode_views(2, &[v]))
            .collect::<Result<_>>()?;
        pairing_verdict(&all)
    }

    fn refused_with(r: Result<()>, what: &str) {
        match r {
            Err(Error::Unsupported(s)) => {
                assert!(s.contains("UD-1105"), "{s}");
                assert!(s.contains(what), "expected {what:?} in {s}");
            }
            other => panic!("expected a refusal naming {what:?}, got {other:?}"),
        }
    }

    // slaves 0..=4 at x = 0, 0.25, …, 1 and their masters 10..=14 at the
    // same abscissae (split nodes)
    const X: [Real; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

    #[test]
    fn strips_keeping_both_sides_together_are_accepted() {
        let r0 = view(
            &[(0, true, X[0]), (1, true, X[1]), (2, false, X[2])],
            &[(10, X[0]), (11, X[1]), (12, X[2])],
            &[(0, 10), (1, 11), (2, 12)],
        );
        let r1 = view(
            &[(2, true, X[2]), (3, true, X[3]), (4, true, X[4])],
            &[(12, X[2]), (13, X[3]), (14, X[4])],
            &[(2, 12), (3, 13), (4, 14)],
        );
        verdict(vec![r0, r1]).unwrap();
    }

    #[test]
    fn separated_sides_are_refused() {
        let slaves: Vec<_> = (0..5).map(|i| (i as i64, true, X[i])).collect();
        let masters: Vec<_> = (0..5).map(|i| (10 + i as i64, X[i])).collect();
        refused_with(
            verdict(vec![view(&slaves, &[], &[]), view(&[], &masters, &[])]),
            "is not paired there",
        );
    }

    #[test]
    fn a_rank_seeing_one_slave_node_is_refused_even_with_the_right_master() {
        let r0 = view(
            &[
                (0, true, X[0]),
                (1, true, X[1]),
                (2, true, X[2]),
                (3, true, X[3]),
            ],
            &[(10, X[0]), (11, X[1]), (12, X[2]), (13, X[3])],
            &[(0, 10), (1, 11), (2, 12), (3, 13)],
        );
        // one slave node: upstream's tolerance is ~1e153 and the first master
        // in group order is taken; here it happens to be the right one
        let r1 = view(&[(4, true, X[4])], &[(14, X[4]), (13, X[3])], &[(4, 14)]);
        refused_with(verdict(vec![r0, r1]), "sees one slave node");
    }

    #[test]
    fn an_arbitrary_master_of_a_lone_slave_is_refused() {
        let r0 = view(
            &[
                (0, true, X[0]),
                (1, true, X[1]),
                (2, true, X[2]),
                (3, true, X[3]),
            ],
            &[(10, X[0]), (11, X[1]), (12, X[2]), (13, X[3])],
            &[(0, 10), (1, 11), (2, 12), (3, 13)],
        );
        let r1 = view(&[(4, true, X[4])], &[(12, X[2]), (14, X[4])], &[(4, 12)]);
        let r = verdict(vec![r0, r1]);
        refused_with(r, "which the pairing of the whole mesh does not pair");
    }

    #[test]
    fn a_pair_of_a_larger_rank_tolerance_is_refused() {
        // the whole mesh: slaves 0 / 1 at 0 and 0.01 give the tolerance
        // 0.001; slave 2 at 0.5 is 0.005 from master 12 and stays unpaired
        let r0 = view(
            &[(0, true, 0.0), (1, true, 0.01)],
            &[(10, 0.0), (11, 0.01)],
            &[(0, 10), (1, 11)],
        );
        // rank 1 sees slaves 1 (ghost) and 2: tolerance 0.049, pairs 2 / 12
        let r1 = view(
            &[(1, false, 0.01), (2, true, 0.5)],
            &[(11, 0.01), (12, 0.505)],
            &[(1, 11), (2, 12)],
        );
        refused_with(
            verdict(vec![r0, r1]),
            "rank 1 pairs slave node 2 with master node 12",
        );
    }

    #[test]
    fn ranks_with_other_surface_pairs_are_refused() {
        let a = view(&[(0, true, 0.0)], &[], &[]);
        let mut b = view(&[(1, true, 1.0)], &[], &[]);
        b.digest = 8;
        let r = verdict(vec![a, b]);
        assert!(
            matches!(&r, Err(Error::InvalidArgument(s)) if s.contains("other surface pairs")),
            "{r:?}"
        );
    }

    #[test]
    fn a_truncated_record_is_an_error() {
        let mut rec = encode_views(2, &[view(&[(0, true, 0.0)], &[], &[])]).unwrap();
        rec.pop();
        assert!(matches!(
            pairing_verdict(&[rec]),
            Err(Error::Communication(_))
        ));
    }

    #[test]
    fn the_global_rule_matches_pair_interface_nodes_order_and_tolerance() {
        let slaves: BTreeMap<i64, (bool, Vec<Real>)> =
            [(0, (false, vec![0.0, 0.0])), (1, (false, vec![0.25, 0.0]))]
                .into_iter()
                .collect();
        // two masters within the tolerance of slave 0: the first in
        // global-id order wins (keep_searching = false), both with true
        let masters: BTreeMap<i64, (bool, Vec<Real>)> = [
            (10, (false, vec![0.001, 0.0])),
            (11, (false, vec![0.0, 0.0])),
            (12, (true, vec![0.25, 0.0])),
        ]
        .into_iter()
        .collect();
        let first = global_pairs(&slaves, &masters, 2, 1, false);
        assert_eq!(first.into_iter().collect::<Vec<_>>(), vec![(0, 10)]);
        let all = global_pairs(&slaves, &masters, 2, 1, true);
        // the periodic master 12 is dropped after the match
        assert_eq!(all.into_iter().collect::<Vec<_>>(), vec![(0, 10), (0, 11)]);
    }
}
