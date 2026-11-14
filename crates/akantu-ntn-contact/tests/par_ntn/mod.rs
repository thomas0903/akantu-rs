// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential scenarios (oracle/probes/par4: ntn_compression,
// ntrf_coulomb; upstream's NTNBaseContact::initParallel() called by the probe,
// D-083), shared by ntn_parallel.rs (local world) and mpi_ntn.rs (real MPI).

//! Split-node contact on distributed meshes (MPI-051, MPI-052): the probe's
//! scenarios on the oracle's strip partition, bitwise at 1, 2 and 4 ranks:
//! mesh, contact nodes, the filtered schemes of `init_parallel`, the
//! `_cf_nodal` / `_cf_incr` exchanges of rank-specific values, then 60
//! explicit steps (contact pressure, normals, friction, displacements,
//! velocities; the all-reduced external work). Every checkpoint's outcome
//! is agreed on every rank before the run continues.
//!
//! `update_normals_after_init_parallel`
//! (case v140_d), `separated_partition_is_refused`
//! (UD-1105) and `contact_sums_failure_is_agreed`.

#![allow(dead_code)]

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::enums::{AnalysisMethod, MeshIoType, SpatialDirection, SynchronizationTag};
use akantu_core::{GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FlagOnly, FromSameDim};
use akantu_ntn_contact::{
    ModelState, NtnBaseContact, NtnContactSolverCallback, NtnFriction, TasnUpstreamCompat,
    initialize_ntn_friction,
};
use akantu_parser::{Parser, SearchScope};
use akantu_solid::SolidMechanicsModel;
use akantu_validation::Fixture;

use super::par_common;

pub type Outcome = std::result::Result<(), String>;

fn ok<T>(r: Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// Material file of the probe's `ntrf_coulomb`.
pub const NTRF_DAT: &str = "friction coulomb rubin_ampuero [\n mu = 0.2\n t_star = 2e-5\n]\n\
material elastic [\n  name = slider\n  rho = 1200\n  nu = 0.33\n  E = 7.98e9\n  Plane_Stress = 1\n]\n\
material elastic [\n  name = base\n  rho = 1200\n  nu = 0.33\n  E = 7.98e9\n  Plane_Stress = 1\n]\n";

pub fn fixture(scenario: &str, comm: &Communicator) -> std::result::Result<Fixture, String> {
    Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{}/rank{}",
        comm.size(),
        comm.rank()
    ))
    .map_err(|e| e.to_string())
}

fn bits(name: &str, got: &[Real], want: &[Real]) -> Outcome {
    if got.len() != want.len() {
        return Err(format!(
            "{name}: {} values, oracle {}",
            got.len(),
            want.len()
        ));
    }
    for (k, (a, b)) in got.iter().zip(want).enumerate() {
        if a.to_bits() != b.to_bits() {
            return Err(format!("{name}[{k}]: {a:e} vs oracle {b:e}"));
        }
    }
    Ok(())
}

fn ints(name: &str, got: &[i64], want: &[i64]) -> Outcome {
    if got != want {
        let k = got
            .iter()
            .zip(want)
            .position(|(a, b)| a != b)
            .unwrap_or(got.len().min(want.len()));
        return Err(format!(
            "{name}: differs at {k} ({:?} vs oracle {:?}; lengths {} / {})",
            got.get(k),
            want.get(k),
            got.len(),
            want.len()
        ));
    }
    Ok(())
}

/// The outcome of a checkpoint agreed on every rank: a rank
/// whose comparison fails no longer leaves the others waiting in the next
/// step's collective until the harness timeout.
pub fn agreed(comm: &Communicator, what: &str, r: Outcome) -> Outcome {
    let mut f = [u64::from(r.is_err())];
    comm.all_reduce(&mut f, ReduceOp::Max)
        .map_err(|e| format!("{what}: agreement: {e}"))?;
    match r {
        Err(e) => Err(e),
        Ok(()) if f[0] != 0 => Err(format!("{what}: failed on another rank")),
        Ok(()) => Ok(()),
    }
}

fn f64q(fx: &Fixture, name: &str) -> std::result::Result<Vec<Real>, String> {
    Ok(fx.load_f64(name).map_err(|e| e.to_string())?.data)
}

fn i64q(fx: &Fixture, name: &str) -> std::result::Result<Vec<i64>, String> {
    Ok(fx.load_i64(name).map_err(|e| e.to_string())?.data)
}

/// `ntn_test_compression.msh` read on rank 0 and distributed with the
/// oracle's strip partition (one rank: the bisection partitioner).
pub fn distributed_mesh(
    comm: &Communicator,
    scenario: &str,
    path: &std::path::Path,
) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(akantu_io::read_mesh(&mut m, path, MeshIoType::Gmsh), "read")?;
    }
    if comm.size() > 1 {
        let parts = if comm.rank() == 0 {
            par_common::oracle_partition(scenario, comm.size())?
        } else {
            Vec::new()
        };
        par_common::distribute_prescribed(&mut m, parts)?;
    } else {
        par_common::distribute_rcb(&mut m)?;
    }
    Ok(m)
}

fn check_mesh(model: &SolidMechanicsModel, fx: &Fixture) -> Outcome {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let d = mesh.is_distributed();
    let gid = |n: Idx| {
        if d {
            mesh.node_global_id(n)
        } else {
            i64::from(n)
        }
    };
    for g in GhostType::ALL {
        for ty in mesh.element_types(akantu_core::TypeFilter::new().dim(2).ghost(g)) {
            let name = format!(
                "init/conn/{ty}/{}",
                if g == GhostType::Ghost {
                    "ghost"
                } else {
                    "not_ghost"
                }
            );
            let want = i64q(fx, &name)?;
            let conn = ok(mesh.connectivity(ty, g), "connectivity")?;
            let got: Vec<i64> = conn.as_slice().iter().map(|&n| gid(n)).collect();
            ints(&name, &got, &want)?;
        }
    }
    let n = mesh.nb_nodes();
    let ids: Vec<i64> = (0..n).map(|k| gid(k as Idx)).collect();
    ints(
        "init/node_global_ids",
        &ids,
        &i64q(fx, "init/node_global_ids")?,
    )
}

fn check_contact(model: &SolidMechanicsModel, c: &NtnBaseContact, fx: &Fixture) -> Outcome {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let d = mesh.is_distributed();
    let gid = |n: Idx| {
        if d {
            mesh.node_global_id(n)
        } else {
            i64::from(n)
        }
    };
    let s: Vec<i64> = c
        .slaves()
        .as_slice()
        .iter()
        .map(|&n| i64::from(n))
        .collect();
    ints("contact/slaves", &s, &i64q(fx, "contact/slaves")?)?;
    let sg: Vec<i64> = c.slaves().as_slice().iter().map(|&n| gid(n)).collect();
    ints(
        "contact/slaves_global",
        &sg,
        &i64q(fx, "contact/slaves_global")?,
    )?;
    if let Some(m) = c.masters() {
        let mg: Vec<i64> = m.as_slice().iter().map(|&n| gid(n)).collect();
        ints(
            "contact/masters_global",
            &mg,
            &i64q(fx, "contact/masters_global")?,
        )?;
    }
    Ok(())
}

fn check_schemes(c: &NtnBaseContact, fx: &Fixture) -> Outcome {
    let Some(sync) = c.synchronizer() else {
        return Ok(());
    };
    for (sr, dir) in [
        (akantu_comm::synchronizer::SendRecv::Send, "send"),
        (akantu_comm::synchronizer::SendRecv::Recv, "recv"),
    ] {
        for (p, scheme) in sync.communications.schemes(sr) {
            let qn = format!("contact/filtered/{dir}/{p}");
            let got: Vec<i64> = scheme
                .iter()
                .flat_map(|e| {
                    [
                        i64::from(e.ty() as u8),
                        e.idx() as i64,
                        e.ghost_type() as i64,
                    ]
                })
                .collect();
            match fx.load_i64(&qn) {
                Ok(want) => ints(&qn, &got, &want.data)?,
                Err(_) if got.is_empty() => {}
                Err(e) => return Err(format!("{qn}: {e}")),
            }
        }
    }
    Ok(())
}

/// The probe's `exchange_check`: rank-specific values at the copies, then
/// `_cf_nodal` and `_cf_incr`, then zeros.
fn exchange_check(model: &mut SolidMechanicsModel, c: &NtnBaseContact, fx: &Fixture) -> Outcome {
    let (gids, owned): (Vec<Real>, Vec<bool>) = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        let d = mesh.is_distributed();
        (0..mesh.nb_nodes())
            .map(|k| {
                let n = k as Idx;
                let g = if d {
                    mesh.node_global_id(n) as Real
                } else {
                    k as Real
                };
                let own = !d || mesh.node_flag(n).is_local_or_master();
                (g, own)
            })
            .unzip()
    };
    let rank = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        mesh.communicator().rank()
    };
    let off_of = |own: bool| {
        if own {
            0.
        } else {
            1000. * (Real::from(rank) + 1.)
        }
    };
    {
        let mut u = ok(model.displacement_mut(), "u")?;
        for (k, uu) in u
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            for (dd, x) in uu.iter_mut().enumerate() {
                *x = 1e-3 * (gids[k] + 0.5 * dd as Real) + off_of(owned[k]);
            }
        }
    }
    {
        let mut v = ok(model.velocity_mut(), "v")?;
        for (k, vv) in v
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            for (dd, x) in vv.iter_mut().enumerate() {
                *x = 2e-3 * (gids[k] - 0.25 * dd as Real) + off_of(owned[k]);
            }
        }
    }
    {
        let mut inc = ok(model.increment_mut(), "inc")?;
        for (k, ii) in inc
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
        {
            for (dd, x) in ii.iter_mut().enumerate() {
                *x = 3e-3 * (gids[k] + 0.125 * dd as Real) + off_of(owned[k]);
            }
        }
    }
    ok(
        c.synchronize_nodal(SynchronizationTag::CfNodal, model),
        "_cf_nodal",
    )?;
    bits(
        "exchange/cf_nodal/displacement",
        ok(model.displacement(), "u")?.as_slice(),
        &f64q(fx, "exchange/cf_nodal/displacement")?,
    )?;
    bits(
        "exchange/cf_nodal/velocity",
        ok(model.velocity(), "v")?.as_slice(),
        &f64q(fx, "exchange/cf_nodal/velocity")?,
    )?;
    bits(
        "exchange/cf_nodal/current_position",
        ok(model.current_position(), "x")?.as_slice(),
        &f64q(fx, "exchange/cf_nodal/current_position")?,
    )?;
    ok(
        c.synchronize_nodal(SynchronizationTag::CfIncr, model),
        "_cf_incr",
    )?;
    bits(
        "exchange/cf_incr/increment",
        ok(model.increment(), "inc")?.as_slice(),
        &f64q(fx, "exchange/cf_incr/increment")?,
    )?;
    ok(model.displacement_mut(), "u")?.as_mut_slice().fill(0.);
    ok(model.velocity_mut(), "v")?.as_mut_slice().fill(0.);
    ok(model.increment_mut(), "inc")?.as_mut_slice().fill(0.);
    Ok(())
}

fn check_state(
    model: &SolidMechanicsModel,
    c: &NtnBaseContact,
    f: &NtnFriction,
    fx: &Fixture,
    pre: &str,
) -> Outcome {
    bits(
        &format!("{pre}/displacement"),
        ok(model.displacement(), "u")?.as_slice(),
        &f64q(fx, &format!("{pre}/displacement"))?,
    )?;
    bits(
        &format!("{pre}/velocity"),
        ok(model.velocity(), "v")?.as_slice(),
        &f64q(fx, &format!("{pre}/velocity"))?,
    )?;
    bits(
        &format!("{pre}/contact_pressure"),
        c.contact_pressure().as_slice(),
        &f64q(fx, &format!("{pre}/contact_pressure"))?,
    )?;
    bits(
        &format!("{pre}/normals"),
        c.normals().as_slice(),
        &f64q(fx, &format!("{pre}/normals"))?,
    )?;
    let ic: Vec<i64> = c
        .is_in_contact()
        .as_slice()
        .iter()
        .map(|&b| i64::from(b))
        .collect();
    ints(
        &format!("{pre}/is_in_contact"),
        &ic,
        &i64q(fx, &format!("{pre}/is_in_contact"))?,
    )?;
    bits(
        &format!("{pre}/friction_traction"),
        f.friction_traction().as_slice(),
        &f64q(fx, &format!("{pre}/friction_traction"))?,
    )?;
    bits(
        &format!("{pre}/slip"),
        f.slip().as_slice(),
        &f64q(fx, &format!("{pre}/slip"))?,
    )?;
    let st: Vec<i64> = f
        .is_sticking()
        .as_slice()
        .iter()
        .map(|&b| i64::from(b))
        .collect();
    ints(
        &format!("{pre}/is_sticking"),
        &st,
        &i64q(fx, &format!("{pre}/is_sticking"))?,
    )
}

fn fixed_bcs(model: &mut SolidMechanicsModel, block_slider_x: bool) -> Outcome {
    let x = FlagOnly {
        axis: SpatialDirection::X,
    };
    let y = FlagOnly {
        axis: SpatialDirection::Y,
    };
    ok(model.apply_dirichlet_bc(&y, "base_bottom"), "bc")?;
    ok(model.apply_dirichlet_bc(&x, "base_bottom"), "bc")?;
    ok(model.apply_dirichlet_bc(&x, "base_left"), "bc")?;
    ok(model.apply_dirichlet_bc(&x, "base_right"), "bc")?;
    if block_slider_x {
        ok(model.apply_dirichlet_bc(&x, "slider_left"), "bc")?;
        ok(model.apply_dirichlet_bc(&x, "slider_right"), "bc")?;
    }
    Ok(())
}

/// Probe `ntn_compression`: bitwise.
pub fn ntn_compression_matches_oracle(comm: Communicator) -> Outcome {
    let fx = fixture("ntn_compression", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "ntn_compression",
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let parser = ok(
        Parser::parse_file(fx.input_path("material").map_err(|e| e.to_string())?),
        "parser",
    )?;
    let tsf: Real = ok(
        parser
            .user_section()
            .and_then(|s| s.get_parameter_value("time_step_factor", SearchScope::Current)),
        "time_step_factor",
    )?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        ),
        "model",
    )?;
    let mut solver = ok(
        NtnContactSolverCallback::new(
            &mut model,
            "slider_bottom",
            "base_top",
            1,
            tsf,
            TasnUpstreamCompat::upstream(),
        ),
        "callback",
    )?;
    agreed(
        &comm,
        "mesh and contact",
        check_mesh(&model, &fx).and_then(|()| check_contact(&model, solver.contact(), &fx)),
    )?;
    {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        ok(solver.contact_mut().init_parallel(&mesh), "init_parallel")?;
    }
    agreed(
        &comm,
        "schemes and contact data",
        (|| {
            check_schemes(solver.contact(), &fx)?;
            bits(
                "contact/normals_init",
                solver.contact().normals().as_slice(),
                &f64q(&fx, "contact/normals_init")?,
            )?;
            bits(
                "contact/impedance",
                solver.contact().impedance().as_slice(),
                &f64q(&fx, "contact/impedance")?,
            )?;
            bits(
                "contact/lumped_boundary_slaves",
                solver.contact().lumped_boundary_slaves().as_slice(),
                &f64q(&fx, "contact/lumped_boundary_slaves")?,
            )
        })(),
    )?;
    let r = exchange_check(&mut model, solver.contact(), &fx);
    agreed(&comm, "exchange check", r)?;
    fixed_bcs(&mut model, true)?;
    let time_step = ok(model.time_step(None), "dt")?;
    agreed(
        &comm,
        "time step",
        f64q(&fx, "time_step").and_then(|w| bits("time_step", &[time_step], &w)),
    )?;
    let period = 5e-5;
    for s in 0..60 {
        let t = s as Real * time_step;
        let trac = if t < period / 2.0 {
            let sn = (t / period * 2.0 * std::f64::consts::PI).sin();
            vec![0.0 * sn, -1e6 * sn]
        } else {
            vec![0.0, 0.0]
        };
        ok(
            model.apply_neumann_bc(&FromSameDim::new(trac), "slider_top"),
            "traction",
        )?;
        ok(solver.solve_step(&mut model, None), "solve_step")?;
        if s % 20 == 19 {
            let pre = format!("step{s}");
            let r = check_state(&model, solver.contact(), solver.friction(), &fx, &pre);
            agreed(&comm, &pre, r)?;
        }
    }
    let local_work = solver.external_work();
    let w = ok(solver.global_external_work(&model), "external work")?;
    agreed(
        &comm,
        "final",
        (|| {
            external_work_matches(&comm, &fx, local_work, w)?;
            bits(
                "final/global_contact_pressure",
                solver.contact().global_contact_pressure().as_slice(),
                &f64q(&fx, "final/global_contact_pressure")?,
            )
        })(),
    )
}

/// `final/external_work` of `ntn_compression`: upstream's
/// `getExternalWork()` is `allReduce(_sum)` of the ranks' works
/// (`MPI_Allreduce`). Where the probe recorded this rank's work (probe
/// version 2, `final/external_work_rank`) it is compared bitwise with the
/// port's, and the recorded global value must equal, bit for bit, the
/// fold of every rank's recorded work in the association observed for the
/// oracle's Open MPI at this rank count
/// ([`par_common::oracle_allreduce_fold`]). The port's global value is
/// compared bitwise; when it differs, the difference must be the summation
/// order of the reduction (D-039, the form of its amendment): under the
/// rank-ordered policy the port's value must equal the rank-order fold of
/// every rank's recorded work exactly; under
/// `ReductionPolicy::NativeMpi` (real MPI, upstream's `MPI_Allreduce`)
/// only the bitwise value is accepted. The port relies on no association
/// of `MPI_Allreduce` (the observed one depends on the rank count: rank
/// order at 2 and 3 ranks, pairwise at 4 and 8).
fn external_work_matches(
    comm: &Communicator,
    fx: &Fixture,
    local: [Real; 2],
    global: [Real; 2],
) -> Outcome {
    let rank_rec = f64q(fx, "final/external_work_rank").ok();
    if let Some(r) = &rank_rec {
        bits("final/external_work_rank", &local, r)?;
    }
    let want = f64q(fx, "final/external_work")?;
    // every rank's recorded work, one column per component
    let np = comm.size();
    let recorded: Option<[Vec<Real>; 2]> = match &rank_rec {
        None => None,
        Some(_) => {
            let mut cols = [Vec::with_capacity(np), Vec::with_capacity(np)];
            for r in 0..np {
                let fr = Fixture::open_by_id(&format!("parallel/ntn_compression/np{np}/rank{r}"))
                    .map_err(|e| e.to_string())?;
                let c = f64q(&fr, "final/external_work_rank")?;
                if c.len() != 2 {
                    return Err(format!(
                        "rank {r}: final/external_work_rank has {} values",
                        c.len()
                    ));
                }
                for k in 0..2 {
                    cols[k].push(c[k]);
                }
            }
            Some(cols)
        }
    };
    if let Some(cols) = &recorded {
        let mut observed = [0.0; 2];
        for k in 0..2 {
            observed[k] = par_common::oracle_allreduce_fold(&cols[k])
                .map_err(|e| format!("final/external_work: {e}"))?;
        }
        bits(
            "final/external_work recorded vs the fold of the recorded per-rank works in the \
             association observed for the oracle's MPI_Allreduce",
            &want,
            &observed,
        )?;
    }
    let same = global.len() == want.len()
        && global
            .iter()
            .zip(&want)
            .all(|(a, b)| a.to_bits() == b.to_bits());
    if same {
        return Ok(());
    }
    let cols = match &recorded {
        Some(cols) if comm.reduction_policy() != akantu_comm::ReductionPolicy::NativeMpi => cols,
        _ => return bits("final/external_work", &global, &want),
    };
    let mut fold = [0.0; 2];
    for k in 0..2 {
        fold[k] = par_common::rank_order_fold(&cols[k])
            .ok_or_else(|| "final/external_work_rank: no rank".to_string())?;
    }
    bits(
        "final/external_work (rank-order fold of the recorded per-rank works, D-039)",
        &global,
        &fold,
    )?;
    if comm.rank() == 0 {
        eprintln!(
            "INFO ntn_compression np{np}: external work {global:?} = rank-order fold of the \
             recorded per-rank works; upstream's MPI_Allreduce gives {want:?} = their fold in \
             the association observed for the oracle's Open MPI (D-039)"
        );
    }
    Ok(())
}

/// Probe `ntrf_coulomb`: bitwise.
pub fn ntrf_coulomb_matches_oracle(comm: Communicator) -> Outcome {
    let fx = fixture("ntrf_coulomb", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "ntrf_coulomb",
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let parser = ok(Parser::parse_str(NTRF_DAT, "ntrf.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        ),
        "model",
    )?;
    let mesh_h = model.physics.base.mesh.clone();
    let compat = TasnUpstreamCompat::upstream();
    let mut contact = {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(
            NtnBaseContact::new_ntrf(&m, 2, "contact", compat),
            "contact",
        )?
    };
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )?;
    ok(model.assemble_mass_lumped(), "mass")?;
    ok(contact.set_reference_point(0.0, 0.0, 0.0), "ref")?;
    ok(contact.set_normal(0.0, 1.0, 0.0), "normal")?;
    {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.add_surface(&st, "slider_bottom"), "surface")?;
    }
    let dt = ok(model.stable_time_step(), "dt")? * 0.1;
    ok(model.set_time_step(dt, None), "dt")?;
    let friction = {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.update_impedance(&st), "impedance")?;
        drop(st);
        ok(
            initialize_ntn_friction(&contact, m.nb_nodes(), &model.physics.base.parser),
            "friction",
        )?
    };
    agreed(
        &comm,
        "mesh and contact",
        check_mesh(&model, &fx).and_then(|()| check_contact(&model, &contact, &fx)),
    )?;
    {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(contact.init_parallel(&m), "init_parallel")?;
    }
    agreed(&comm, "schemes", check_schemes(&contact, &fx))?;
    let r = exchange_check(&mut model, &contact, &fx);
    agreed(&comm, "exchange check", r)?;
    let mut cb = NtnContactSolverCallback::from_parts(contact, friction);
    fixed_bcs(&mut model, false)?;
    ok(
        model.apply_neumann_bc(&FromSameDim::new(vec![0.0, -1e6]), "slider_top"),
        "traction",
    )?;
    agreed(
        &comm,
        "time step",
        f64q(&fx, "time_step").and_then(|w| bits("time_step", &[dt], &w)),
    )?;
    for s in 0..60 {
        if s >= 40 {
            ok(
                model.apply_neumann_bc(&FromSameDim::new(vec![4e3, 0.0]), "slider_top"),
                "traction",
            )?;
        }
        ok(cb.solve_step(&mut model, None), "solve_step")?;
        if s % 20 == 19 {
            let pre = format!("step{s}");
            let r = check_state(&model, cb.contact(), cb.friction(), &fx, &pre);
            agreed(&comm, &pre, r)?;
        }
    }
    Ok(())
}

/// UD-904 (category A): on a distributed mesh, a contact step without
/// `init_parallel` is refused on every rank (the step's outcome is agreed),
/// promptly, and the communicator stays usable; on one rank it runs.
pub fn refused_without_init_parallel(comm: Communicator) -> Outcome {
    let np = comm.size();
    // the strip partition of the oracle when recorded, else bisection
    let path = fixture("ntn_compression", &comm)
        .ok()
        .and_then(|f| f.input_path("mesh").ok())
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/data/ntn_test_compression.msh")
        });
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(
            akantu_io::read_mesh(&mut m, &path, MeshIoType::Gmsh),
            "read",
        )?;
    }
    par_common::distribute_rcb(&mut m)?;
    let parser = ok(Parser::parse_str(NTRF_DAT, "ntrf.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(SharedMesh::new(m), None, "solid_mechanics_model", parser),
        "model",
    )?;
    let mesh_h = model.physics.base.mesh.clone();
    let mut contact = {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(
            NtnBaseContact::new_ntrf(&m, 2, "contact", TasnUpstreamCompat::default()),
            "contact",
        )?
    };
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )?;
    ok(model.assemble_mass_lumped(), "mass")?;
    ok(contact.set_normal(0.0, 1.0, 0.0), "normal")?;
    {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.add_surface(&st, "slider_bottom"), "surface")?;
    }
    let dt = ok(model.stable_time_step(), "dt")? * 0.1;
    ok(model.set_time_step(dt, None), "dt")?;
    let friction = {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.update_impedance(&st), "impedance")?;
        drop(st);
        ok(
            initialize_ntn_friction(&contact, m.nb_nodes(), &model.physics.base.parser),
            "friction",
        )?
    };
    let mut cb = NtnContactSolverCallback::from_parts(contact, friction);
    let t0 = std::time::Instant::now();
    let r = cb.solve_step(&mut model, None);
    let distributed = {
        let m = ok(mesh_h.read(), "mesh")?;
        m.synchronizers().element.is_some() && np > 1
    };
    match (distributed, r) {
        (false, r) => ok(r, "one-rank step"),
        (true, Ok(())) => Err("a distributed step without init_parallel ran".into()),
        (true, Err(e)) => {
            let s = e.to_string();
            if !(s.contains("UD-904") || s.contains("failed on another rank")) {
                return Err(format!("unexpected error: {s}"));
            }
            if t0.elapsed() > std::time::Duration::from_secs(10) {
                return Err("the refusal was not prompt".into());
            }
            // the communicator is usable
            let mut one = [1u64];
            ok(
                comm.all_reduce(&mut one, akantu_comm::ReduceOp::Sum),
                "all_reduce",
            )?;
            if one[0] != np as u64 {
                return Err("the communicator is not usable after the refusal".into());
            }
            Ok(())
        }
    }
}

/// Rank-specific failure inside a distributed contact step (the last rank
/// holds a contact pair of nodes that do not exist): every rank's step
/// fails at the same agreement, promptly, the communicator usable. The bad
/// node is added before `init_parallel` (after it, every addition is
/// refused on a distributed mesh; the filtered
/// synchronizer ignores a node outside every element), so the first step
/// fails.
pub fn contact_step_failure_is_agreed(comm: Communicator) -> Outcome {
    if comm.size() == 1 {
        return Ok(());
    }
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/ntn_test_compression.msh");
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(
            akantu_io::read_mesh(&mut m, &path, MeshIoType::Gmsh),
            "read",
        )?;
    }
    par_common::distribute_rcb(&mut m)?;
    let parser = ok(Parser::parse_str(NTRF_DAT, "ntrf.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(SharedMesh::new(m), None, "solid_mechanics_model", parser),
        "model",
    )?;
    let mesh_h = model.physics.base.mesh.clone();
    let mut contact = {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(
            NtnBaseContact::new_ntrf(&m, 2, "contact", TasnUpstreamCompat::default()),
            "contact",
        )?
    };
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )?;
    ok(model.assemble_mass_lumped(), "mass")?;
    ok(contact.set_normal(0.0, 1.0, 0.0), "normal")?;
    {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.add_surface(&st, "slider_bottom"), "surface")?;
    }
    let dt = ok(model.stable_time_step(), "dt")? * 0.1;
    ok(model.set_time_step(dt, None), "dt")?;
    let friction = {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.update_impedance(&st), "impedance")?;
        drop(st);
        let friction = ok(
            initialize_ntn_friction(&contact, m.nb_nodes(), &model.physics.base.parser),
            "friction",
        )?;
        if comm.rank() as usize == comm.size() - 1 {
            ok(contact.add_split_node(1 << 30, 1 << 30), "bad node")?;
        }
        ok(contact.init_parallel(&m), "init_parallel")?;
        friction
    };
    let mut cb = NtnContactSolverCallback::from_parts(contact, friction);
    let t0 = std::time::Instant::now();
    // D-106 (2): the rank-local verdict is agreed before the all-reduce
    let local = match cb.solve_step(&mut model, None) {
        Ok(()) => Err("a rank-specific failure was not reported on this rank".into()),
        Err(_) if t0.elapsed() > std::time::Duration::from_secs(10) => {
            Err("the failure was not prompt".into())
        }
        Err(_) => Ok(()),
    };
    match agreed(&comm, "rank-specific step failure", local) {
        Err(e) => Err(e),
        Ok(()) => {
            let mut one = [1u64];
            ok(
                comm.all_reduce(&mut one, akantu_comm::ReduceOp::Sum),
                "all_reduce",
            )?;
            if one[0] != comm.size() as u64 {
                return Err("the communicator is not usable".into());
            }
            Ok(())
        }
    }
}

/// The probe's `ntn_compression` model on `mesh` with the slip-weakening
/// friction of `material` (the solver callback pairs the surfaces on each
/// rank, before `init_parallel`).
fn compression_model(
    mesh: Mesh,
    material: &std::path::Path,
) -> std::result::Result<(SolidMechanicsModel, NtnContactSolverCallback), String> {
    let parser = ok(Parser::parse_file(material), "parser")?;
    let tsf: Real = ok(
        parser
            .user_section()
            .and_then(|s| s.get_parameter_value("time_step_factor", SearchScope::Current)),
        "time_step_factor",
    )?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        ),
        "model",
    )?;
    let solver = ok(
        NtnContactSolverCallback::new(
            &mut model,
            "slider_bottom",
            "base_top",
            1,
            tsf,
            TasnUpstreamCompat::upstream(),
        ),
        "callback",
    )?;
    Ok((model, solver))
}

/// The probe's 40 first loading steps of `ntn_compression`.
fn compression_steps(
    model: &mut SolidMechanicsModel,
    solver: &mut NtnContactSolverCallback,
    nb: usize,
) -> Outcome {
    fixed_bcs(model, true)?;
    let time_step = ok(model.time_step(None), "dt")?;
    let period = 5e-5;
    for s in 0..nb {
        let t = s as Real * time_step;
        let trac = if t < period / 2.0 {
            let sn = (t / period * 2.0 * std::f64::consts::PI).sin();
            vec![0.0 * sn, -1e6 * sn]
        } else {
            vec![0.0, 0.0]
        };
        ok(
            model.apply_neumann_bc(&FromSameDim::new(trac), "slider_top"),
            "traction",
        )?;
        ok(solver.solve_step(model, None), "solve_step")?;
    }
    Ok(())
}

/// Components of `a` and `b` whose bits differ, and the largest |a − b|.
fn changed(a: &[Real], b: &[Real]) -> (u64, Real) {
    let mut n = 0u64;
    let mut w: Real = 0.;
    for (x, y) in a.iter().zip(b) {
        if x.to_bits() != y.to_bits() {
            n += 1;
            w = w.max((x - y).abs());
        }
    }
    (n, w)
}

/// Case `v140_d`: upstream's
/// `NTNContact::updateNormals()` starts with `synchronize(_cf_nodal)`
/// (`ntn_contact.cc:239-240`). The probe's `ntn_compression` setting on the
/// oracle's strip partition, `init_parallel`, 40 steps; then:
/// * the read-only `update_normals(&ModelState)` is refused on more than
///   one rank (it cannot exchange), and runs on one rank;
/// * the reference: one explicit `_cf_nodal` exchange, then the normals of
///   a second contact on the same surfaces that has no synchronizer (its
///   `update_normals` exchanges nothing); then the state before the
///   exchange (displacement, velocity, current position) is restored;
/// * `update_normals_of` gives bitwise the reference normals and leaves
///   bitwise the state of the explicit exchange (exactly one exchange; the
///   exchange is not idempotent: a node of scheme elements on both sides
///   swaps its two copies at each exchange).
///
/// At 4 ranks the normals without the exchange differ from the reference
/// (10 components measured, up to 1.6e-14): checked, so the
/// comparison is not vacuous there.
pub fn update_normals_after_init_parallel(comm: Communicator) -> Outcome {
    let fx = fixture("ntn_compression", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "ntn_compression",
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let (mut model, mut solver) =
        compression_model(mesh, &fx.input_path("material").map_err(|e| e.to_string())?)?;
    let mesh_h = model.physics.base.mesh.clone();
    {
        let mesh = ok(mesh_h.read(), "mesh")?;
        ok(solver.contact_mut().init_parallel(&mesh), "init_parallel")?;
    }
    compression_steps(&mut model, &mut solver, 40)?;
    // the read-only form: refused on more than one rank, the same outcome
    // on every rank without communication
    let read_only = {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        solver.contact_mut().update_normals(&st)
    };
    match (comm.size() > 1, read_only) {
        (true, Ok(())) => {
            return Err(
                "update_normals(&ModelState) ran after init_parallel on more than one rank".into(),
            );
        }
        (true, Err(e)) if !e.to_string().contains("update_normals_of") => {
            return Err(format!("unexpected refusal: {e}"));
        }
        (false, Err(e)) => return Err(format!("update_normals on one rank: {e}")),
        _ => {}
    }
    // a contact on the same surfaces without a synchronizer
    let mut reference = {
        let m = ok(mesh_h.read(), "mesh")?;
        let mut c = ok(
            NtnBaseContact::new_ntn(&m, 2, "reference", TasnUpstreamCompat::upstream()),
            "reference contact",
        )?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(
            c.add_surface_pair(&st, "slider_bottom", "base_top", 1),
            "reference surfaces",
        )?;
        c
    };
    let normals_of = |c: &mut NtnBaseContact, model: &mut SolidMechanicsModel| {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, model), "state")?;
        ok(c.update_normals(&st), "reference normals")?;
        Ok::<_, String>(c.normals().as_slice().to_vec())
    };
    let n_without = normals_of(&mut reference, &mut model)?;
    let u0 = ok(model.displacement(), "u")?.as_slice().to_vec();
    let v0 = ok(model.velocity(), "v")?.as_slice().to_vec();
    let x0 = ok(model.current_position(), "x")?.as_slice().to_vec();
    ok(
        solver
            .contact()
            .synchronize_nodal(SynchronizationTag::CfNodal, &mut model),
        "_cf_nodal",
    )?;
    let n_reference = normals_of(&mut reference, &mut model)?;
    let u1 = ok(model.displacement(), "u")?.as_slice().to_vec();
    let x1 = ok(model.current_position(), "x")?.as_slice().to_vec();
    ok(model.displacement_mut(), "u")?
        .as_mut_slice()
        .copy_from_slice(&u0);
    ok(model.velocity_mut(), "v")?
        .as_mut_slice()
        .copy_from_slice(&v0);
    // the exchange writes the current position too (upstream through a
    // const_cast): restore it as well
    ok(model.physics.current_position_mut(), "x")?
        .as_mut_slice()
        .copy_from_slice(&x0);
    ok(
        solver.contact_mut().update_normals_of(&mut model),
        "update_normals_of",
    )?;
    let n_port = solver.contact().normals().as_slice().to_vec();
    let u2 = ok(model.displacement(), "u")?.as_slice().to_vec();
    let x2 = ok(model.current_position(), "x")?.as_slice().to_vec();
    let (nn, wn) = changed(&n_port, &n_reference);
    let (nu, wu) = changed(&u2, &u1);
    let (nx, wx) = changed(&x2, &x1);
    let (nw, ww) = changed(&n_without, &n_reference);
    let mut counts = [nn, nu, nx, nw];
    ok(comm.all_reduce(&mut counts, ReduceOp::Sum), "sum")?;
    let mut ws = [wn, wu, wx, ww];
    ok(comm.all_reduce(&mut ws, ReduceOp::Max), "max")?;
    if comm.rank() == 0 {
        eprintln!(
            "INFO update_normals_after_init_parallel np{}: normals without the exchange differ from \
             the reference in {} components (max {:.3e})",
            comm.size(),
            counts[3],
            ws[3]
        );
    }
    if counts[..3] != [0, 0, 0] {
        return Err(format!(
            "update_normals_of vs one explicit _cf_nodal exchange then the normals: {} normal \
             components differ (max {:e}); the state differs in {} displacement and {} \
             current-position components (max {:e} / {:e})",
            counts[0], ws[0], counts[1], counts[2], ws[1], ws[2]
        ));
    }
    if comm.size() == 4 && counts[3] == 0 {
        return Err("4 ranks: the exchange changes no normal (vacuous comparison)".into());
    }
    if n_port.iter().any(|v| !v.is_finite()) {
        return Err("a normal is not finite".into());
    }
    Ok(())
}

/// Probe `ntn_update_normals`: the `ntn_compression` setting
/// without the exchange check, `init_parallel`, 40 steps (states at steps
/// 19 and 39 bitwise), then upstream's `NTNContact::updateNormals()`, which
/// starts with `synchronize(_cf_nodal)`: the port's `update_normals_of`
/// gives bitwise the oracle's normals, displacement and current position.
pub fn ntn_update_normals_matches_oracle(comm: Communicator) -> Outcome {
    let fx = fixture("ntn_update_normals", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "ntn_update_normals",
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let (mut model, mut solver) =
        compression_model(mesh, &fx.input_path("material").map_err(|e| e.to_string())?)?;
    agreed(
        &comm,
        "contact",
        check_contact(&model, solver.contact(), &fx),
    )?;
    {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        ok(solver.contact_mut().init_parallel(&mesh), "init_parallel")?;
    }
    fixed_bcs(&mut model, true)?;
    let time_step = ok(model.time_step(None), "dt")?;
    agreed(
        &comm,
        "time step",
        f64q(&fx, "time_step").and_then(|w| bits("time_step", &[time_step], &w)),
    )?;
    let period = 5e-5;
    for s in 0..40 {
        let t = s as Real * time_step;
        let trac = if t < period / 2.0 {
            let sn = (t / period * 2.0 * std::f64::consts::PI).sin();
            vec![0.0 * sn, -1e6 * sn]
        } else {
            vec![0.0, 0.0]
        };
        ok(
            model.apply_neumann_bc(&FromSameDim::new(trac), "slider_top"),
            "traction",
        )?;
        ok(solver.solve_step(&mut model, None), "solve_step")?;
        if s % 20 == 19 {
            let pre = format!("step{s}");
            let r = check_state(&model, solver.contact(), solver.friction(), &fx, &pre);
            agreed(&comm, &pre, r)?;
        }
    }
    ok(
        solver.contact_mut().update_normals_of(&mut model),
        "update_normals_of",
    )?;
    let normals = solver.contact().normals().as_slice().to_vec();
    let u = ok(model.displacement(), "u")?.as_slice().to_vec();
    let x = ok(model.current_position(), "x")?.as_slice().to_vec();
    agreed(
        &comm,
        "update_normals",
        (|| {
            bits(
                "update_normals/normals",
                &normals,
                &f64q(&fx, "update_normals/normals")?,
            )?;
            bits(
                "update_normals/displacement",
                &u,
                &f64q(&fx, "update_normals/displacement")?,
            )?;
            bits(
                "update_normals/current_position",
                &x,
                &f64q(&fx, "update_normals/current_position")?,
            )
        })(),
    )
}

/// A prescribed partition of `ntn_test_compression.msh` that separates the
/// two sides of the interface: the base (barycentre y < 0) on the first
/// half of the ranks, the slider on the second half, each half in vertical
/// strips. No element of one side touches the other (split nodes), so no
/// rank holds both nodes of an interface pair.
fn separated_parts(m: &Mesh, np: usize) -> std::result::Result<Vec<Idx>, String> {
    let half = (np / 2).max(1);
    let mut parts = Vec::new();
    for ty in m.element_types(
        akantu_core::TypeFilter::new()
            .dim(2)
            .ghost(GhostType::NotGhost),
    ) {
        let b = ok(m.barycenters(ty, GhostType::NotGhost), "barycenters")?;
        for x in b.as_slice().as_chunks::<2>().0 {
            let strip = ((x[0] * half as Real).floor().max(0.0) as usize).min(half - 1);
            let p = if x[1] < 0.0 {
                strip
            } else {
                (np - half) + strip
            };
            parts.push(Idx::try_from(p.min(np - 1)).map_err(|e| e.to_string())?);
        }
    }
    Ok(parts)
}

/// UD-1105, D-097 (2): on a partition that separates the two
/// sides of the interface, the per-rank pairing forms no pair while the
/// whole mesh pairs every slave node: `init_parallel` refuses with the same
/// `Error::Unsupported` on every rank, promptly, installs no synchronizer,
/// and the communicator stays usable. On one rank there is nothing to
/// refuse.
pub fn separated_partition_is_refused(comm: Communicator) -> Outcome {
    let np = comm.size();
    let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(
            akantu_io::read_mesh(
                &mut m,
                &data.join("ntn_test_compression.msh"),
                MeshIoType::Gmsh,
            ),
            "read",
        )?;
    }
    if np > 1 {
        let parts = if comm.rank() == 0 {
            separated_parts(&m, np)?
        } else {
            Vec::new()
        };
        par_common::distribute_prescribed(&mut m, parts)?;
    } else {
        par_common::distribute_rcb(&mut m)?;
    }
    let (model, mut solver) =
        compression_model(m, &data.join("material_ntn_friction_slip_weakening.dat"))?;
    let nb_local_pairs = solver.contact().nb_contact_nodes();
    let t0 = std::time::Instant::now();
    let r = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        solver.contact_mut().init_parallel(&mesh)
    };
    if np == 1 {
        ok(r, "init_parallel on one rank")?;
        return if nb_local_pairs > 0 {
            Ok(())
        } else {
            Err("one rank: no pair".into())
        };
    }
    // D-106 (2): the rank-local verdict is agreed before the digest
    // reduction below
    let local = match r {
        Ok(()) => Err(format!(
            "init_parallel accepted a partition separating the interface ({nb_local_pairs} pairs on \
             rank {})",
            comm.rank()
        )),
        Err(e) => {
            let s = e.to_string();
            if !matches!(e, akantu_core::Error::Unsupported(_)) || !s.contains("UD-1105") {
                Err(format!("unexpected error: {s}"))
            } else if !s.contains("is not paired there") {
                Err(format!(
                    "the refusal does not name an unpaired slave node: {s}"
                ))
            } else if t0.elapsed() > std::time::Duration::from_secs(10) {
                Err("the refusal was not prompt".into())
            } else if solver.contact().synchronizer().is_some() {
                Err("a synchronizer was installed despite the refusal".into())
            } else {
                Ok(s)
            }
        }
    };
    match local {
        Err(e) => agreed(&comm, "separated partition", Err(e)),
        Ok(s) => {
            agreed(&comm, "separated partition", Ok(()))?;
            // the same message on every rank (computed from the gathered records)
            let mut h = [akantu_comm::phase::digest_bytes(0, s.as_bytes())];
            let mut lo = [!h[0]];
            ok(comm.all_reduce(&mut h, ReduceOp::Max), "max")?;
            ok(comm.all_reduce(&mut lo, ReduceOp::Max), "max")?;
            if h[0] != !lo[0] {
                return Err("the ranks report different refusals".into());
            }
            Ok(())
        }
    }
}

/// The contact's sums are failure-agreed. A rank-local error
/// just before them (an out-of-range slave node on the last rank only for
/// `nb_nodes_in_contact`, an out-of-range direction on the last rank only
/// for `integrate_residual`) returns an error on every rank, promptly, and
/// the communicator stays usable.
pub fn contact_sums_failure_is_agreed(comm: Communicator) -> Outcome {
    if comm.size() == 1 {
        return Ok(());
    }
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/ntn_test_compression.msh");
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(
            akantu_io::read_mesh(&mut m, &path, MeshIoType::Gmsh),
            "read",
        )?;
    }
    par_common::distribute_rcb(&mut m)?;
    let parser = ok(Parser::parse_str(NTRF_DAT, "ntrf.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(SharedMesh::new(m), None, "solid_mechanics_model", parser),
        "model",
    )?;
    let mesh_h = model.physics.base.mesh.clone();
    let mut contact = {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(
            NtnBaseContact::new_ntrf(&m, 2, "contact", TasnUpstreamCompat::default()),
            "contact",
        )?
    };
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )?;
    ok(model.assemble_mass_lumped(), "mass")?;
    ok(contact.set_normal(0.0, 1.0, 0.0), "normal")?;
    {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.add_surface(&st, "slider_bottom"), "surface")?;
    }
    let last = comm.rank() as usize == comm.size() - 1;
    // healthy sums first
    {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(contact.nb_nodes_in_contact(&m), "nb_nodes_in_contact")?;
    }
    ok(
        akantu_ntn_contact::boundary_functions::integrate_residual("slider_top", &model, 1),
        "integrate_residual",
    )?;
    if last {
        ok(contact.add_split_node(1 << 30, 0), "bad node")?;
    }
    let usable = |what: &str| -> Outcome {
        let mut one = [1u64];
        ok(comm.all_reduce(&mut one, ReduceOp::Sum), what)?;
        if one[0] != comm.size() as u64 {
            return Err(format!("{what}: the communicator is not usable"));
        }
        Ok(())
    };
    let t0 = std::time::Instant::now();
    let r = {
        let m = ok(mesh_h.read(), "mesh")?;
        contact.nb_nodes_in_contact(&m)
    };
    if r.is_ok() {
        return Err("nb_nodes_in_contact: a rank-local error was not reported on this rank".into());
    }
    if t0.elapsed() > std::time::Duration::from_secs(10) {
        return Err("nb_nodes_in_contact: the failure was not prompt".into());
    }
    usable("after nb_nodes_in_contact")?;
    let t0 = std::time::Instant::now();
    let dir = if last { 5 } else { 1 };
    let r = akantu_ntn_contact::boundary_functions::integrate_residual("slider_top", &model, dir);
    if r.is_ok() {
        return Err("integrate_residual: a rank-local error was not reported on this rank".into());
    }
    if t0.elapsed() > std::time::Duration::from_secs(10) {
        return Err("integrate_residual: the failure was not prompt".into());
    }
    usable("after integrate_residual")
}

/// After `init_parallel` on a mesh distributed over
/// more than one rank, **every** addition of contact nodes is refused
/// before anything changes, with the same recoverable
/// `Error::Unsupported` on every rank: the pair additions
/// (`add_surface_pair`, `add_node_pairs`, also with an empty list), the
/// low-level `add_split_node` they use, and the rigid-flat `add_surface`
/// (also for a group the mesh lacks) and `add_nodes` (with a
/// rank-dependent list, empty on rank 0). The contact keeps its nodes and
/// its synchronizer. On one rank (no exchange) the same additions run.
/// Both contacts on the oracle's strip partitions (`ntn_compression`,
/// `ntrf_coulomb`).
pub fn additions_after_init_parallel_are_refused(comm: Communicator) -> Outcome {
    let np = comm.size();
    // the same refusal text on every rank (no rank-local wording)
    let same_everywhere = |what: &str, s: &str| -> Outcome {
        let mut h = [akantu_comm::phase::digest_bytes(0, s.as_bytes())];
        let mut lo = [!h[0]];
        ok(comm.all_reduce(&mut h, ReduceOp::Max), "max")?;
        ok(comm.all_reduce(&mut lo, ReduceOp::Max), "max")?;
        if h[0] == !lo[0] {
            Ok(())
        } else {
            Err(format!("{what}: the ranks report different refusals"))
        }
    };
    // D-106 (2): the rank-local verdict is agreed before the next
    // collective (the digest comparison), so a rank whose check fails does
    // not leave the others waiting there
    let check = |what: &str, r: Result<()>, before: &[Idx], c: &NtnBaseContact| -> Outcome {
        if np == 1 {
            return agreed(&comm, what, ok(r, &format!("{what} on one rank")));
        }
        let local = match r {
            Ok(()) => Err(format!("{what} accepted after init_parallel on {np} ranks")),
            Err(e) => {
                let s = e.to_string();
                if !matches!(e, akantu_core::Error::Unsupported(_))
                    || e.fatal_kind().is_some()
                    || !s.contains("after init_parallel()")
                {
                    Err(format!("{what}: unexpected error: {s}"))
                } else if c.slaves().as_slice() != before || c.synchronizer().is_none() {
                    Err(format!("{what}: the contact changed despite the refusal"))
                } else {
                    Ok(s)
                }
            }
        };
        let text = match local {
            Ok(s) => {
                agreed(&comm, what, Ok(()))?;
                s
            }
            Err(e) => return agreed(&comm, what, Err(e)),
        };
        same_everywhere(what, &text)
    };

    // node-to-node: the probe's compression setting
    let fx = fixture("ntn_compression", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "ntn_compression",
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let (mut model, mut solver) =
        compression_model(mesh, &fx.input_path("material").map_err(|e| e.to_string())?)?;
    let mesh_h = model.physics.base.mesh.clone();
    {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(solver.contact_mut().init_parallel(&m), "init_parallel")?;
    }
    let pair = (0 as Idx, 1 as Idx);
    for what in ["addSurfacePair", "addNodePairs", "addSplitNode"] {
        let before = solver.contact().slaves().as_slice().to_vec();
        let r = {
            let m = ok(mesh_h.read(), "mesh")?;
            let st = ok(ModelState::of_model(&m, &mut model), "state")?;
            let c = solver.contact_mut();
            match what {
                "addSurfacePair" => c.add_surface_pair(&st, "slider_bottom", "base_top", 1),
                "addNodePairs" => {
                    // empty on rank 0: the refusal does not depend on the list
                    let pairs: &[(Idx, Idx)] = if comm.rank() == 0 { &[] } else { &[pair] };
                    c.add_node_pairs(&st, pairs)
                }
                _ => c.add_split_node(pair.0, pair.1),
            }
        };
        check(what, r, &before, solver.contact())?;
    }

    // rigid flat: the probe's ntrf_coulomb setting
    let fx = fixture("ntrf_coulomb", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "ntrf_coulomb",
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let parser = ok(Parser::parse_str(NTRF_DAT, "ntrf.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        ),
        "model",
    )?;
    let mesh_h = model.physics.base.mesh.clone();
    let mut contact = {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(
            NtnBaseContact::new_ntrf(&m, 2, "contact", TasnUpstreamCompat::upstream()),
            "contact",
        )?
    };
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )?;
    ok(model.assemble_mass_lumped(), "mass")?;
    ok(contact.set_reference_point(0.0, 0.0, 0.0), "ref")?;
    ok(contact.set_normal(0.0, 1.0, 0.0), "normal")?;
    {
        let m = ok(mesh_h.read(), "mesh")?;
        let st = ok(ModelState::of_model(&m, &mut model), "state")?;
        ok(contact.add_surface(&st, "slider_bottom"), "surface")?;
    }
    {
        let m = ok(mesh_h.read(), "mesh")?;
        ok(contact.init_parallel(&m), "init_parallel")?;
    }
    for what in [
        "addSurface",
        "addSurface(missing)",
        "addNodes",
        "addSplitNode",
    ] {
        let before = contact.slaves().as_slice().to_vec();
        let r = {
            let m = ok(mesh_h.read(), "mesh")?;
            let st = ok(ModelState::of_model(&m, &mut model), "state")?;
            match what {
                "addSurface" => contact.add_surface(&st, "base_top"),
                "addSurface(missing)" => contact.add_surface(&st, "no_such_group"),
                "addNodes" => {
                    let nodes: &[Idx] = if comm.rank() == 0 { &[] } else { &[0] };
                    contact.add_nodes(&st, nodes)
                }
                _ => contact.add_split_node(0, 0),
            }
        };
        check(what, r, &before, &contact)?;
    }
    Ok(())
}
