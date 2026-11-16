// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Distributed cases of
// the set-up suite: the ports of src/mesh/mesh.cc (Mesh::distribute),
// src/mesh_utils/mesh_utils_distribution.cc, src/synchronizer/
// (node and element synchronizers, facet synchronizer, communicator
// reductions) and a distributed explicit solid step
// (src/model/solid_mechanics/solid_mechanics_model.cc).

//! Distributed cases (any rank count; `mpirun -n P`, or `--local-ranks P`
//! for the in-process world of `akantu-comm`). Rank 0 builds the mesh and
//! partitions it (Scotch when the binary has the `scotch` feature, else
//! RCB); the distribution and everything after it is collective. At one
//! rank the mesh stays serial (as the cohesive driver keeps it): the
//! exchanges then have no peer.
//!
//! Digests: the distribution cases hash every rank's local mesh and combine
//! the per-rank digests in rank order (a distributed mesh depends on the
//! rank count by construction); the exchange and step cases gather the
//! owned nodes' values in global-id order on rank 0.

use akantu_comm::buffer::CommunicationBuffer;
use akantu_comm::{DataAccessor, ReduceDataAccessor, ReduceOp, SimpleIdxDataAccessor};
use akantu_core::enums::{AnalysisMethod, SynchronizationTag};
use akantu_core::{Element, ElementType, Error, GhostType, Real, Result, TypeFilter};
use akantu_mesh::partition::{MeshPartition, MeshPartitioner};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::Model as _;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

use super::cases_mesh::tet4_n;
use super::harness::{CaseOut, Ctx, Digest};
use super::meshgen::{self, Grid};
use super::{mesh_digest, nb_volume_elements};

fn partitioner() -> Box<dyn MeshPartitioner> {
    #[cfg(feature = "scotch")]
    {
        Box::new(akantu_mesh::scotch::ScotchPartitioner::new(0))
    }
    #[cfg(not(feature = "scotch"))]
    {
        Box::new(akantu_mesh::partition::RcbPartitioner)
    }
}

/// Phases `mesh` (rank 0 reads the generated cube), `partition` (rank 0)
/// and `distribute` (collective); returns the local mesh and the global
/// element and node counts.
fn distributed_tet4(ctx: &Ctx, out: &mut CaseOut, n: usize) -> Result<Mesh> {
    let comm = ctx.comm.clone();
    let root = comm.rank() == 0;
    let mut mesh = out.phase("mesh", || {
        let mut m = Mesh::with_communicator(3, "mesh", comm.clone())?;
        if root {
            let text = meshgen::msh22(Grid::Tet4, n);
            akantu_io::read_mesh_from_str(
                &mut m,
                "suite.msh",
                &text,
                akantu_core::enums::MeshIoType::Gmsh,
                &akantu_io::ReadOptions::default(),
            )?;
        }
        Ok(m)
    })?;
    let mut counts = [0u64; 2];
    if root {
        counts = [nb_volume_elements(&mesh) as u64, mesh.nb_nodes() as u64];
    }
    comm.broadcast(&mut counts, 0)?;
    out.n_elements = usize::try_from(counts[0]).unwrap_or(0);
    out.n_nodes = usize::try_from(counts[1]).unwrap_or(0);
    if comm.size() == 1 {
        out.phases.push(super::harness::Phase {
            name: "partition",
            calls: 1,
            seconds: 0.0,
        });
        out.phases.push(super::harness::Phase {
            name: "distribute",
            calls: 1,
            seconds: 0.0,
        });
        return Ok(mesh);
    }
    let partition: Option<Result<MeshPartition>> = out.phase("partition", || {
        Ok(if root {
            Some(partitioner().partition(&mesh, comm.size()))
        } else {
            None
        })
    })?;
    let partition = partition.transpose()?;
    out.phase("distribute", || {
        akantu_mesh::distribution::distribute(&mut mesh, partition.as_ref(), 0).map(|_| ())
    })?;
    Ok(mesh)
}

fn local_mesh_digest(m: &Mesh) -> u64 {
    let mut d = Digest::new();
    mesh_digest(m, &mut d);
    for n in 0..m.nb_nodes() {
        if let Ok(i) = akantu_core::try_idx(n) {
            d.u64(m.node_global_id(i) as u64);
            d.bytes(&[m.node_flag(i).is_local_or_master() as u8]);
        }
    }
    d.value()
}

/// Partition and distribution.
pub fn dist_mesh_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mesh = distributed_tet4(ctx, &mut out, tet4_n(ctx))?;
    out.digest = local_mesh_digest(&mesh);
    Ok(out)
}

/// Facet mesh of a distributed mesh.
pub fn dist_facets_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mesh = distributed_tet4(ctx, &mut out, tet4_n(ctx))?;
    let shared = SharedMesh::new(mesh);
    out.phase("init_mesh_facets", || {
        shared
            .setup(akantu_mesh::utils::init_mesh_facets)
            .map(|_| ())
    })?;
    let m = shared.read()?;
    let mut d = Digest::new();
    d.u64(local_mesh_digest(&m));
    if let Some(fm) = m.mesh_facets() {
        mesh_digest(fm, &mut d);
    }
    out.digest = d.value();
    Ok(out)
}

/// The owned (local or master) nodes' values of `data` (`nc` per node),
/// gathered on rank 0 in global-id order and hashed; the digest is
/// broadcast to every rank.
fn owned_nodal_digest(ctx: &Ctx, m: &Mesh, data: &[Real], nc: usize) -> Result<u64> {
    let mut gids: Vec<i64> = Vec::new();
    let mut vals: Vec<f64> = Vec::new();
    for n in 0..m.nb_nodes() {
        let i = akantu_core::try_idx(n)?;
        if m.node_flag(i).is_local_or_master() {
            gids.push(m.node_global_id(i));
            vals.extend_from_slice(&data[n * nc..(n + 1) * nc]);
        }
    }
    let all_g = ctx.comm.gather_v(&gids, 0)?;
    let all_v = ctx.comm.gather_v(&vals, 0)?;
    let mut digest = [0u64];
    if let (Some(g), Some(v)) = (all_g, all_v) {
        let mut rows: Vec<(i64, &[f64])> = Vec::new();
        for (gr, vr) in g.iter().zip(v.iter()) {
            for (k, &gid) in gr.iter().enumerate() {
                rows.push((gid, &vr[k * nc..(k + 1) * nc]));
            }
        }
        rows.sort_by_key(|r| r.0);
        let mut d = Digest::new();
        for (gid, v) in rows {
            d.u64(gid as u64);
            d.reals(v);
        }
        digest[0] = d.value();
    }
    ctx.comm.broadcast(&mut digest, 0)?;
    Ok(digest[0])
}

fn initial_nodal(m: &Mesh, nc: usize) -> Vec<Real> {
    let mut v = vec![0.0; m.nb_nodes() * nc];
    for n in 0..m.nb_nodes() {
        let gid = akantu_core::try_idx(n).map_or(0, |i| m.node_global_id(i));
        for c in 0..nc {
            #[allow(clippy::cast_precision_loss)] // small ids
            {
                v[n * nc + c] = 1.0 + (gid as f64) * 0.5 + c as f64 * 0.25;
            }
        }
    }
    v
}

/// The residual pattern of the DOF manager: slave reduction (sum) then
/// synchronize (copy) on the node synchronizer, 3 reals per node.
pub fn ghost_nodes_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mesh = distributed_tet4(ctx, &mut out, tet4_n(ctx))?;
    let nc = 3;
    let init = initial_nodal(&mesh, nc);
    let mut data = init.clone();
    let sync = mesh.synchronizers().node.clone();
    let exchange = |data: &mut Vec<Real>| -> Result<()> {
        if let Some(s) = &sync {
            s.slave_reduction(
                &mut ReduceDataAccessor::new(data, nc, SynchronizationTag::SmmRes, ReduceOp::Sum),
                SynchronizationTag::SmmRes,
            )?;
            s.synchronize(
                &mut SimpleIdxDataAccessor::new(data, nc, SynchronizationTag::SmmRes),
                SynchronizationTag::SmmRes,
            )?;
        }
        Ok(())
    };
    let calls = ctx.size.pick(5, 5000, 2000);
    // every call exchanges the initial values (a sum repeated on its own
    // result would grow without bound over the calls); the copy is a small
    // share of the call
    out.loop_phase(ctx, "reduce_synchronize", calls, || {
        data.copy_from_slice(&init);
        exchange(&mut data)
    })?;
    // the last call's result: one exchange from the initial values
    // (independent of the number of calls)
    out.digest = owned_nodal_digest(ctx, &mesh, &data, nc)?;
    out.digest_global = true;
    let shared: usize = sync
        .as_ref()
        .map_or(0, |s| s.communications.send.values().map(Vec::len).sum());
    #[allow(clippy::cast_precision_loss)] // a count
    out.extra.push(("shared_nodes_rank0_send", shared as f64));
    Ok(out)
}

/// Element data accessor: `nc` reals per element of the mesh dimension,
/// one block per (type, ghost type), found by a linear search over the few
/// blocks (no hashing per element: the case measures the synchronizer).
struct ElementData {
    nc: usize,
    data: Vec<((ElementType, GhostType), Vec<Real>)>,
}

impl ElementData {
    fn block(&self, e: &Element) -> Option<&Vec<Real>> {
        let key = (e.ty(), e.ghost_type());
        self.data.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
    fn block_mut(&mut self, e: &Element) -> Option<&mut Vec<Real>> {
        let key = (e.ty(), e.ghost_type());
        self.data
            .iter_mut()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }
    fn record(&self, e: &Element) -> Result<&[Real]> {
        let i = usize::try_from(e.index()).map_err(|_| Error::invalid("element index"))?;
        self.block(e)
            .and_then(|v| v.get(i * self.nc..(i + 1) * self.nc))
            .ok_or_else(|| Error::invalid("element out of range"))
    }
}

impl DataAccessor<Element> for ElementData {
    fn nb_data(&self, entities: &[Element], _: SynchronizationTag) -> usize {
        akantu_comm::accessor::record_bytes::<Real>(entities.len(), self.nc)
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        for e in entities {
            buffer.pack_slice(self.record(e)?);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        entities: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        let nc = self.nc;
        for e in entities {
            let i = usize::try_from(e.index()).map_err(|_| Error::invalid("element index"))?;
            let v = self
                .block_mut(e)
                .and_then(|v| v.get_mut(i * nc..(i + 1) * nc))
                .ok_or_else(|| Error::invalid("element out of range"))?;
            buffer.unpack_slice(v)?;
        }
        Ok(())
    }
}

/// Owner → ghost exchange of 9 reals per element.
pub fn ghost_elements_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mesh = distributed_tet4(ctx, &mut out, tet4_n(ctx))?;
    let nc = 9;
    let mut acc = ElementData {
        nc,
        data: Vec::new(),
    };
    for ghost in [GhostType::NotGhost, GhostType::Ghost] {
        for ty in mesh.element_types(TypeFilter::new().dim(3).ghost(ghost)) {
            let ne = mesh.nb_element(ty, ghost);
            let v: Vec<Real> = (0..ne * nc)
                .map(|k| {
                    if ghost == GhostType::NotGhost {
                        #[allow(clippy::cast_precision_loss)] // small
                        {
                            k as f64 * 1e-3 + ctx.rank() as f64
                        }
                    } else {
                        -1.0
                    }
                })
                .collect();
            acc.data.push(((ty, ghost), v));
        }
    }
    let sync = mesh.synchronizers().element.clone();
    let calls = ctx.size.pick(5, 5000, 2000);
    out.loop_phase(ctx, "synchronize", calls, || match &sync {
        Some(s) => s.synchronize(&mut acc, SynchronizationTag::SmmStress),
        None => Ok(()),
    })?;
    let mut d = Digest::new();
    for ((ty, ghost), v) in {
        let mut k: Vec<_> = acc.data.iter().map(|(key, v)| (key, v)).collect();
        k.sort_by_key(|((t, g), _)| (format!("{t:?}"), *g == GhostType::Ghost));
        k
    } {
        d.bytes(format!("{ty:?}{ghost:?}").as_bytes());
        d.reals(v);
    }
    out.digest = d.value();
    Ok(out)
}

/// Reductions.
pub fn reduce_scalars(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    #[allow(clippy::cast_precision_loss)] // small
    let r = ctx.rank() as f64;
    let mut two = [0.0f64; 2];
    let calls2 = ctx.size.pick(20, 200_000, 200_000);
    out.loop_phase(ctx, "all_reduce_2", calls2, || {
        two = [r + 0.5, 2.0 * r + 0.25];
        ctx.comm.all_reduce(&mut two, ReduceOp::Sum)
    })?;
    let mut big = vec![0.0f64; 4096];
    let calls_big = ctx.size.pick(5, 20_000, 20_000);
    out.loop_phase(ctx, "all_reduce_4096", calls_big, || {
        for (k, v) in big.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)] // small
            {
                *v = r + k as f64 * 1e-3;
            }
        }
        ctx.comm.all_reduce(&mut big, ReduceOp::Sum)
    })?;
    let mut d = Digest::new();
    d.reals(&two).reals(&big);
    out.digest = d.value();
    out.digest_global = true;
    Ok(out)
}

/// Steel, as the explicit_dynamic example.
pub const STEEL: &str =
    "material elastic [\n    name = steel\n    rho = 7800\n    E   = 2.1e11\n    nu  = 0.3\n]\n";

/// Sinus × Gaussian pulse of the explicit_dynamic example, on the unit
/// cube.
pub fn pulse(x: Real) -> Real {
    let l = 0.25;
    let a = 1e-4;
    let x = x - 0.5;
    let k = 0.1 * 2. * std::f64::consts::PI * 3. / l;
    a * (k * x).sin() * (-(k * x) * (k * x) / (l * l)).exp()
}

/// Explicit elastic model on `mesh` with the pulse as initial
/// displacement and 0.8 × the stable time step.
pub fn explicit_model(mesh: Mesh, out: &mut CaseOut) -> Result<SolidMechanicsModel> {
    let dim = mesh.spatial_dimension();
    let nodes = mesh.nodes().as_slice().to_vec();
    let mut model = out.phase("init", || {
        let parser = Parser::parse_str(STEEL, "material.dat")?;
        let mut model = SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        )?;
        model.init_full(&akantu_model::ModelOptions::new(
            AnalysisMethod::ExplicitLumpedMass,
        ))?;
        let dt = model.stable_time_step()? * 0.8;
        model.set_time_step(dt, None)?;
        Ok(model)
    })?;
    {
        let mut u = model.displacement_mut()?;
        let u = u.as_mut_slice();
        for (n, x) in nodes.chunks_exact(dim).enumerate() {
            u[n * dim] = pulse(x[0]);
        }
    }
    Ok(model)
}

/// Extra fields of a loop phase: agreements of the collective error
/// protocol per call (rank 0) and the microseconds per call spent in them,
/// waiting included, on the fastest and the slowest rank
/// (`akantu_comm::phase::agreement_stats`).
fn agreement_extras(
    ctx: &Ctx,
    out: &mut CaseOut,
    before: (u64, f64),
    calls: usize,
    what: &'static str,
) -> Result<()> {
    let after = akantu_comm::phase::agreement_stats();
    #[allow(clippy::cast_precision_loss)] // counts
    let per = |x: f64| x / calls.max(1) as f64;
    #[allow(clippy::cast_precision_loss)] // counts
    let n = per((after.0 - before.0) as f64);
    let us = per(after.1 - before.1) * 1e6;
    let (mut lo, mut hi) = ([us], [us]);
    ctx.comm.all_reduce(&mut lo, ReduceOp::Min)?;
    ctx.comm.all_reduce(&mut hi, ReduceOp::Max)?;
    let names: [&'static str; 3] = match what {
        "step" => [
            "agreements_per_step",
            "agree_us_per_step_min",
            "agree_us_per_step_max",
        ],
        _ => [
            "agreements_per_energies",
            "agree_us_per_energies_min",
            "agree_us_per_energies_max",
        ],
    };
    out.extra.push((names[0], n));
    out.extra.push((names[1], lo[0]));
    out.extra.push((names[2], hi[0]));
    Ok(())
}

/// Distributed explicit step and the per-step energies.
pub fn explicit_step_dist_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let n = ctx.size.pick(6, 30, 60);
    let mesh = distributed_tet4(ctx, &mut out, n)?;
    let mut model = explicit_model(mesh, &mut out)?;
    out.n_dofs = out.n_nodes * 3;
    let steps = ctx.size.pick(5, 100, 50);
    let mut calls = 0usize;
    let before = akantu_comm::phase::agreement_stats();
    out.loop_phase(ctx, "solve_step", steps, || {
        calls += 1;
        model.solve_step(None)
    })?;
    agreement_extras(ctx, &mut out, before, calls, "step")?;
    let mut e = [0.0; 2];
    let mut calls = 0usize;
    let before = akantu_comm::phase::agreement_stats();
    out.loop_phase(ctx, "energies", ctx.size.pick(5, 200, 100), || {
        calls += 1;
        e = [model.energy("potential")?, model.energy("kinetic")?];
        Ok(())
    })?;
    agreement_extras(ctx, &mut out, before, calls, "energies")?;
    let mesh_h = model.mesh().clone();
    let m = mesh_h.read()?;
    let u = model.displacement()?;
    let mut d = Digest::new();
    d.u64(owned_nodal_digest(ctx, &m, u.as_slice(), 3)?);
    out.digest = d.value();
    out.digest_global = true;
    Ok(out)
}
