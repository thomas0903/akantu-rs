// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the persistent state the collective error protocol leaves
// after a failure (D-126) in the distributed heat-transfer model, on the
// in-process local world.
//
// When a solve step returns an error, every rank's persistent state (the
// temperature and its rate, the lumped capacity, the laws' internal
// variables, the DOF manager's blocked snapshot) must be the state the
// every-phase mode leaves on that rank for the same injected failure. The
// explicit (lumped capacity) and implicit solve steps are checked with a
// failure injected in each of their phases in turn (every agreement,
// deferred outcome and collective of the step, as recorded by a run without
// failure), on the first and the last rank at 2 ranks, the boundary
// conditions changed before the step (so that its blocked snapshot
// changes) and the capacity reassembled in it.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_diffusion;

use std::time::Duration;

use akantu_comm::phase::{CollectiveChecks, testing};
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_parser::Parser;
use par_diffusion::ok;

type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[f64]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn state(model: &HeatTransferModel) -> Result<State, String> {
    let mut out = State::new();
    out.push((
        "temperature".into(),
        bits(ok(model.temperature(), "T")?.as_slice()),
    ));
    out.push((
        "temperature rate".into(),
        bits(ok(model.temperature_rate(), "rate")?.as_slice()),
    ));
    if let Ok(c) = model.lumped_capacity() {
        out.push(("lumped capacity".into(), bits(c.as_slice())));
    }
    for l in 0..model.nb_laws() {
        let law = ok(model.law(l), "law")?;
        law.for_each_internal(&mut |name, f| {
            let InternalFieldRef::Real(f) = f else {
                return;
            };
            let values = f.values().to_plain().unwrap();
            for (ty, ghost, a) in values.as_map().iter_all() {
                let key = format!("law {l}/{name}/{ty:?}/{ghost:?}");
                out.push((key.clone(), bits(a.as_slice())));
                if f.has_history() {
                    out.push((
                        format!("{key}/previous"),
                        bits(f.previous(ty, ghost).unwrap().as_slice()),
                    ));
                }
            }
        });
    }
    let dm = model.dof_manager();
    out.push((
        "blocked snapshot".into(),
        dm.global_blocked_dofs_indexes()
            .iter()
            .map(|&i| i as u64)
            .chain(dm.global_blocked_dofs().iter().map(|&b| u64::from(b)))
            .chain([
                dm.blocked_release(),
                u64::from(dm.has_blocked_dofs_changed()),
            ])
            .collect(),
    ));
    Ok(out)
}

struct Seen {
    error: Option<String>,
    state: State,
    keys: Vec<String>,
}

/// Two steps, then the boundary changed and the capacity marked for
/// reassembly, and the third step with `inject` armed on rank `failing`
/// (`None`: its phase keys recorded).
fn run(
    comm: &Communicator,
    method: AnalysisMethod,
    mode: CollectiveChecks,
    inject: Option<(&str, u32)>,
    failing: usize,
) -> Result<Seen, String> {
    comm.set_collective_checks(mode);
    let mesh = par_diffusion::distributed_cube(comm, 3)?;
    let parser = ok(Parser::parse_str(par_diffusion::DAT, "heat.dat"), "parser")?;
    let shared = SharedMesh::new(mesh);
    let mut model = ok(
        HeatTransferModel::with_parser(shared.clone(), None, "heat_transfer", parser),
        "model",
    )?;
    let mut options = ModelOptions::new(method);
    if method != AnalysisMethod::ExplicitLumpedMass {
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    ok(model.init_full(&options), "init_full")?;
    let (coords, nb) = {
        let m = ok(shared.read(), "read")?;
        (m.nodes().as_slice().to_vec(), m.nb_nodes())
    };
    {
        let mut t_g = ok(model.temperature_mut(), "T")?;
        let t = t_g.as_mut_slice();
        for n in 0..nb {
            let (x, y, z) = (coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
            t[n] = 20. + 30. * x * (1. - y) + 10. * z * z;
        }
    }
    for n in 0..nb {
        if coords[3 * n + 2] == 0.0 {
            ok(model.blocked_dofs_mut(), "blocked")?.as_mut_slice()[n] = true;
            ok(model.temperature_mut(), "T")?.as_mut_slice()[n] = 100.;
        }
    }
    let dt = ok(model.stable_time_step(), "stable time step")?;
    let dt = if method == AnalysisMethod::ExplicitLumpedMass {
        0.5 * dt
    } else {
        20. * dt
    };
    ok(model.set_time_step(dt, None), "dt")?;
    if method != AnalysisMethod::ExplicitLumpedMass {
        use akantu_parser::param::ParamValue;
        let nls = ok(model.solver.non_linear_solver_mut(None), "nls")?;
        ok(
            nls.set_param("max_iterations", ParamValue::Int(30)),
            "max_it",
        )?;
        ok(
            nls.set_param("threshold", ParamValue::Real(1e-10)),
            "threshold",
        )?;
    }
    for s in 0..2 {
        ok(model.solve_step(None), &format!("solve step {s}"))?;
    }
    for n in 0..nb {
        if coords[3 * n] == 1.0 {
            ok(model.blocked_dofs_mut(), "blocked")?.as_mut_slice()[n] = true;
        }
    }
    model.force_capacity_reassembly();
    let me = usize::try_from(comm.rank()).unwrap() == failing;
    match inject {
        Some((p, n)) if me => testing::fail_phase(p, n),
        Some(_) => {}
        None => testing::record_keys(),
    }
    let r = model.solve_step(None);
    let keys = testing::recorded_keys();
    testing::clear();
    Ok(Seen {
        error: r.err().map(|e| e.to_string()),
        state: state(&model)?,
        keys,
    })
}

/// For every key of `every`, the occurrence to inject in each mode (the
/// injection matches by substring; a commit point's key may contain it).
fn points(every: &[String], commit: &[String]) -> Vec<(String, u32, u32)> {
    let nth = |keys: &[String], k: &str, exact: usize| -> u32 {
        let j = keys
            .iter()
            .enumerate()
            .filter(|(_, o)| o.as_str() == k)
            .nth(exact)
            .map(|(j, _)| j)
            .unwrap_or_else(|| panic!("phase {k} missing in a run"));
        u32::try_from(keys[..=j].iter().filter(|o| o.contains(k)).count()).unwrap()
    };
    every
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let exact = every[..i].iter().filter(|o| *o == k).count();
            (k.clone(), nth(every, k, exact), nth(commit, k, exact))
        })
        .collect()
}

/// This rank's comparison; every rank runs every injection whatever it
/// finds (differences are collected).
fn check_rank(comm: &Communicator, method: AnalysisMethod) -> Result<usize, String> {
    let rank = usize::try_from(comm.rank()).unwrap();
    let last = comm.size() - 1;
    let mut errors = Vec::new();
    let reference = run(comm, method, CollectiveChecks::EveryPhase, None, usize::MAX)?;
    let control = run(
        comm,
        method,
        CollectiveChecks::CommitPoints,
        None,
        usize::MAX,
    )?;
    if reference.error.is_some() || control.error.is_some() {
        errors.push(format!(
            "rank {rank}: no failure injected: {:?} / {:?}",
            reference.error, control.error
        ));
    }
    if reference.state != control.state {
        errors.push(format!("rank {rank}: no failure: the states differ"));
    }
    let pts = points(&reference.keys, &control.keys);
    let mut n = 0;
    for failing in [0, last] {
        for (pattern, ne, nc) in &pts {
            let every = run(
                comm,
                method,
                CollectiveChecks::EveryPhase,
                Some((pattern, *ne)),
                failing,
            )?;
            let commit = run(
                comm,
                method,
                CollectiveChecks::CommitPoints,
                Some((pattern, *nc)),
                failing,
            )?;
            let w = format!("{method:?}: {pattern} #{ne}, rank {failing} failing, rank {rank}");
            if every.error.is_none() || commit.error.is_none() {
                errors.push(format!("{w}: ended without an error"));
            }
            for ((k, a), (_, b)) in every.state.iter().zip(&commit.state) {
                if a != b {
                    errors.push(format!("{w}: '{k}' differs"));
                }
            }
            if every.state.len() != commit.state.len() {
                errors.push(format!("{w}: different state layouts"));
            }
            n += 1;
        }
    }
    if n < 10 {
        errors.push(format!("{method:?}: only {n} injections"));
    }
    if errors.is_empty() {
        Ok(n)
    } else {
        Err(errors.join("\n"))
    }
}

fn check(method: AnalysisMethod) {
    let out = run_local(2, Duration::from_secs(300), |comm| {
        check_rank(&comm, method)
    });
    for (rank, r) in out.into_iter().enumerate() {
        let n = r.unwrap_or_else(|e| panic!("{e}"));
        if rank == 0 {
            eprintln!("{method:?}: {n} injected failures compared");
        }
    }
}

#[test]
fn explicit_step_failure_leaves_the_every_phase_state() {
    check(AnalysisMethod::ExplicitLumpedMass);
}

#[test]
fn implicit_step_failure_leaves_the_every_phase_state() {
    check(AnalysisMethod::ImplicitDynamic);
}
