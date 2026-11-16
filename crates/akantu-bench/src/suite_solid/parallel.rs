// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Parallel cases of
// the benchmark suite (mesh built on rank 0 and distributed, upstream
// `mesh.distribute()`); the problems are the serial explicit and extrinsic
// cohesive cases of this suite.

//! Cases that run on the MPI world (`mpirun -n P suite_solid ...`, binary
//! built with the `mpi` feature; serial otherwise): the explicit elastic
//! step and the extrinsic cohesive run with insertions, with the same
//! phases as their serial twins plus `distribute`.
//!
//! Digests gather the owned nodes' `u` and `v` to rank 0 in global-id
//! order. They repeat from run to run at a fixed rank count; they differ
//! between rank counts, because the sums at shared nodes (and, with
//! insertions, the numbering of doubled nodes) depend on the partition.

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Idx, Result};
use akantu_mesh::Mesh;
use akantu_mesh::distribution::{MeshPartitioner, distribute_with};
use akantu_solid::SolidMechanicsModel;

use super::common;
use super::mesh::{BoxSpec, box_mesh};
use crate::suite::{Ctx, Digest, Extra, Outcome, gather_owned_by_global_id};

fn partitioner() -> (Box<dyn MeshPartitioner>, &'static str) {
    #[cfg(feature = "scotch")]
    {
        (
            Box::new(akantu_mesh::scotch::ScotchPartitioner::new(0)),
            "scotch",
        )
    }
    #[cfg(not(feature = "scotch"))]
    {
        (Box::new(akantu_mesh::distribution::RcbPartitioner), "rcb")
    }
}

/// The box on rank 0, distributed over the world (phases `mesh`,
/// `distribute`); serial when the world has one rank.
pub(crate) fn distributed_box(ctx: &mut Ctx<'_>, spec: &BoxSpec) -> Result<Mesh> {
    let comm = ctx.comm().clone();
    let root = comm.rank() == 0;
    let mut mesh = ctx.once("mesh", || box_mesh(spec, Some(&comm), root))?;
    if comm.size() > 1 {
        let (mut p, name) = partitioner();
        ctx.once("distribute", || {
            distribute_with(&mut mesh, if root { Some(p.as_mut()) } else { None }, 0).map(|_| ())
        })?;
        ctx.extra("partitioner", Extra::Text(name.into()));
    }
    Ok(mesh)
}

/// Owned counts (elements of the mesh dimension, nodes, DOFs) and the
/// gathered digest of `u` and `v` (`u` alone without a velocity).
pub(crate) fn counts_and_digest(ctx: &mut Ctx<'_>, m: &SolidMechanicsModel) -> Result<()> {
    let d = m.spatial_dimension();
    let (global, owned, ne) = {
        let mesh = m.physics.base.mesh.read()?;
        let n = mesh.nb_nodes();
        let mut global = Vec::with_capacity(n);
        let mut owned = Vec::with_capacity(n);
        for i in 0..n {
            let id = Idx::try_from(i).map_err(|_| akantu_core::Error::invalid("node index"))?;
            global.push(mesh.node_global_id(id) as u64);
            owned.push(mesh.is_local_or_master_node(id)?);
        }
        let ne: usize = mesh
            .element_types(
                akantu_core::TypeFilter::new()
                    .dim(d)
                    .ghost(GhostType::NotGhost),
            )
            .filter(|t| t.kind() == akantu_core::ElementKind::Regular)
            .map(|t| mesh.nb_element(t, GhostType::NotGhost))
            .sum();
        (global, owned, ne)
    };
    let nb_owned = owned.iter().filter(|&&o| o).count();
    ctx.set_counts(ne, nb_owned, nb_owned * d);
    let comm = ctx.comm().clone();
    let u = gather_owned_by_global_id(&comm, &global, &owned, m.displacement()?.as_slice(), d)?;
    // static models have no velocity (the same on every rank)
    let v = match m.velocity() {
        Ok(v) => gather_owned_by_global_id(&comm, &global, &owned, v.as_slice(), d)?,
        Err(_) => None,
    };
    if let Some(u) = u {
        let mut h = Digest::new();
        h.reals(&u);
        if let Some(v) = v {
            h.reals(&v);
        }
        ctx.set_digest(&h);
    }
    Ok(())
}

/// Explicit elastic step (small strain, tetrahedron_4, the serial
/// `explicit_tet4_ss` problem) on the distributed cube.
///
/// Phases: `mesh`, `distribute`, `init`, `solve_step` with its sub-phases
/// (the residual includes the ghost exchanges of the internal forces),
/// `energies`.
///
/// # Errors
/// Model and communicator errors.
pub fn explicit_tet4_ss(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    let n = ctx.pick(4, 30, 60);
    let steps = ctx.pick(4, 200, 200);
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    let mesh = distributed_box(ctx, &spec)?;
    let mut m = ctx.once("init", || {
        let mut m = common::solid_model(mesh, &common::elastic_dat(false))?;
        common::init(&mut m, AnalysisMethod::ExplicitLumpedMass)?;
        Ok(m)
    })?;
    let dt = m.stable_time_step()?;
    m.set_time_step(0.8 * dt, None)?;
    common::apply_pulse(&mut m, 0.0, 1.0)?;
    common::timed_steps(ctx, &mut m, steps, |_| Ok(()))?;
    ctx.repeat("energies", (steps / 5).max(1), |_| {
        std::hint::black_box(m.kinetic_energy()?);
        std::hint::black_box(m.energy("potential")?);
        Ok(())
    })?;
    counts_and_digest(ctx, &m).map(|()| Outcome::Done)
}

/// The extrinsic cohesive run with insertions (`cohesive_extrinsic_insertion`)
/// on the distributed cube.
///
/// Phases: `mesh`, `distribute`, `init`, `preload`, `step`,
/// `check_insertion` (with the distributed insertion: facet
/// synchronisation, global numbering of new nodes), `solve_step`.
///
/// # Errors
/// Model and communicator errors.
pub fn cohesive_extrinsic(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    let cfg = super::cohesive::InsertionRun {
        n: [4, 16, 32],
        ..super::cohesive::INSERTION
    };
    let n = ctx.pick(cfg.n[0], cfg.n[1], cfg.n[2]);
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    let mesh = distributed_box(ctx, &spec)?;
    let m = super::cohesive::insertion_run(ctx, &cfg, mesh)?;
    let mut nc = [super::cohesive::nb_elements(&m, akantu_core::ElementKind::Cohesive)? as u64];
    ctx.comm().all_reduce(&mut nc, akantu_comm::ReduceOp::Sum)?;
    ctx.extra("n_cohesive", Extra::Int(nc[0] as i64));
    counts_and_digest(ctx, &m.smm).map(|()| Outcome::Done)
}
