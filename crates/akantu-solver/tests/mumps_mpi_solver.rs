// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS):
// test/test_model/test_common/test_model_solver/{test_model_solver.cc
// (test_model_solver_mumps), test_model_solver_dynamic.cc
// (test_model_solver_dynamic_implicit, MUMPS), test_dof_manager_default.cc
// (SparseSolverType::_mumps)} with their .verified outputs, and the
// partitioned MyModel toy of crates/akantu-solver/tests/
// solver_collective_protocol.rs (test_model_solver_my_model.hh) on real MPI.

//! Real-MPI acceptance of the MUMPS backend through the DOF manager and the
//! model solver, `harness = false`: run under `mpirun -np
//! 1/2/4` by `scripts/mumps/run-mumps-tests.sh` (feature `mumps`).
//!
//! * the upstream MUMPS model-solver tests (1 process: the toy mesh is
//!   serial): printed output equal to the `.verified` file (upstream's
//!   criterion; those files were produced with MUMPS);
//! * first, `dbltrace_mpi`: the Newton `dblTrace`
//!   dumps on a distributed bar, one file per rank, against the oracle at
//!   2 processes;
//! * the partitioned bar on 1/2/4 ranks with `auto` (→ MUMPS on an MPI
//!   communicator): the installed solver is MUMPS, iteration counts equal
//!   the serial Eigen run's, the gathered `u, u̇, ü` agree with it to
//!   `1e-12` relative to their maximum (justification:
//!   `docs/NUMERICAL_VALIDATION.md`).

mod common;
// The dblTrace dumps on a distributed mesh (first scenario).
mod dbltrace_mpi;
// The distributed test systems of the MUMPS acceptance tests.
#[path = "../../akantu-linalg/tests/mumps_common/mod.rs"]
mod mumps_common;

use akantu_validation::reduce::NanMinMax;
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, Synchronizer};
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Array, GhostType, Idx, NodeFlag, Real};
use akantu_linalg::mumps::{MumpsOptions, MumpsSolver};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::param::ParamValue;
use akantu_solver::{
    DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback, UpstreamCompat,
};

use common::drivers::run_dynamic_on;
use common::my_model::{Law, MyModel, SEG, gen_mesh};
use common::oracle::*;

type Outcome = std::result::Result<(), String>;

macro_rules! ensure {
    ($c:expr, $($fmt:tt)+) => {
        let ok: bool = $c;
        if !ok {
            return Err(format!($($fmt)+));
        }
    };
}

/// A named scenario of the binary.
type Scenario = (&'static str, fn(&Communicator) -> Outcome);

const N: usize = 17; // 16 elements: 16, 8, 4 per rank at 1, 2, 4 ranks

/// A scenario that does not run at this rank count says so
/// (the tier log must not read as a multi-rank pass).
fn not_applicable(c: &Communicator, why: &str) {
    if c.rank() == 0 {
        println!("INFO not applicable at {} ranks: {why}", c.size());
    }
}

fn world() -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(600)),
        ..MpiOptions::default()
    })
    .expect("MPI world")
}

/// Rank `r` of `p` holds global nodes `[r·e, (r+1)·e]` (`e = (N−1)/p`); the
/// boundary node is master on the lower rank and slave on the upper one.
fn partitioned_bar(comm: &Communicator) -> (Mesh, Vec<i64>) {
    let (r, p) = (comm.rank() as usize, comm.size());
    let e = (N - 1) / p;
    let first = r * e;
    let mut mesh = if p == 1 {
        Mesh::new(1, "bar").unwrap()
    } else {
        Mesh::with_communicator(1, "bar", comm.clone()).unwrap()
    };
    let mut gids = Vec::new();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for i in 0..=e {
            let g = first + i;
            let n = acc.push_node(&[g as f64 * (1.0 / (N - 1) as f64)]).unwrap();
            gids.push(g as i64);
            let flag = if p > 1 && i == 0 && r > 0 {
                NodeFlag::SLAVE
            } else if p > 1 && i == e && r + 1 < p {
                NodeFlag::MASTER
            } else {
                NodeFlag::NORMAL
            };
            acc.nodes_flags_mut().as_mut_slice()[n as usize] = flag;
        }
        let mut c = acc.connectivity_mut(SEG, GhostType::NotGhost);
        for i in 0..e {
            c.push_tuple(&[i as Idx, i as Idx + 1]);
        }
        drop(c);
        if p > 1 {
            let mut sync = Synchronizer::new("nodes", comm.clone());
            if r + 1 < p {
                sync.communications
                    .send
                    .insert(r as i32 + 1, vec![e as Idx]);
            }
            if r > 0 {
                sync.communications.recv.insert(r as i32 - 1, vec![0]);
                acc.set_node_prank(0, r as i32 - 1);
            }
            acc.synchronizers_mut().node = Some(sync);
            acc.set_distributed();
        }
    }
    (mesh, gids)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setup {
    StaticNewton,
    StaticLinearSolver,
    StaticCubicNewton,
    ImplicitNewmark,
    ExplicitLumped,
}

/// Build the toy and its solver on `comm` (sinusoidal initial state for the
/// dynamic setups, computed locally from the global coordinate).
fn build(comm: &Communicator, setup: Setup, sparse: SparseSolverType) -> (MyModel, ModelSolver) {
    let (mesh, gids) = partitioned_bar(comm);
    let x: Vec<f64> = mesh.nodes().as_slice().to_vec();
    let mut dm = DofManager::new("dm", comm.clone());
    let lumped = setup == Setup::ExplicitLumped;
    let force = match setup {
        Setup::StaticCubicNewton => -2.0,
        Setup::StaticNewton | Setup::StaticLinearSolver => -10.0,
        _ => 0.0,
    };
    let mut model =
        MyModel::with_global_ids(force, mesh, lumped, &mut dm, gids.clone(), N as i64).unwrap();
    if setup == Setup::StaticCubicNewton {
        model.law = Law::Cubic;
    }
    let dynamic = matches!(setup, Setup::ImplicitNewmark | Setup::ExplicitLumped);
    if dynamic {
        {
            let mut b_g = dm.blocked_dofs_mut("disp").unwrap();
            let b = b_g.as_mut_slice();
            for (k, g) in gids.iter().enumerate() {
                b[k] = *g == 0 || *g == N as i64 - 1;
            }
        }
        let k = 2.0 * std::f64::consts::PI / 0.2;
        let u: Vec<f64> = x.iter().map(|&xi| 0.01 * (k * xi).cos()).collect();
        let v: Vec<f64> = x.iter().map(|&xi| k * 0.01 * (k * xi).sin()).collect();
        dm.dofs_mut("disp")
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&u);
        dm.dofs_derivative_mut("disp", 1)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&v);
    }
    let mut ms = ModelSolver::new(dm);
    let (tss, nls) = match setup {
        Setup::StaticNewton | Setup::StaticCubicNewton => (
            TimeStepSolverType::Static,
            NonLinearSolverType::NewtonRaphson,
        ),
        Setup::StaticLinearSolver => (TimeStepSolverType::Static, NonLinearSolverType::Linear),
        Setup::ImplicitNewmark => (
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
        ),
        Setup::ExplicitLumped => (
            TimeStepSolverType::DynamicLumped,
            NonLinearSolverType::Lumped,
        ),
    };
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(tss),
        non_linear_solver_type: Some(nls),
        sparse_solver_type: Some(sparse),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &opts, &mut model).unwrap();
    let (scheme, st) = match setup {
        Setup::ImplicitNewmark => (
            IntegrationSchemeType::TrapezoidalRule2,
            SolutionType::Displacement,
        ),
        Setup::ExplicitLumped => (
            IntegrationSchemeType::CentralDifference,
            SolutionType::Acceleration,
        ),
        _ => (IntegrationSchemeType::PseudoTime, SolutionType::NotDefined),
    };
    ms.set_integration_scheme("s", "disp", scheme, st, &model)
        .unwrap();
    ms.set_time_step(0.001, None).unwrap();
    if nls == NonLinearSolverType::NewtonRaphson {
        ms.non_linear_solver_mut(None)
            .unwrap()
            .set_param("max_iterations", ParamValue::Int(20))
            .unwrap();
    }
    (model, ms)
}

/// Histories of `steps` solve steps: per step `(iterations, error)` and the
/// final `(u, u̇, ü)` keyed by global node id.
type Histories = (Vec<(usize, Real)>, Vec<(i64, [Real; 3])>);

fn run(
    comm: &Communicator,
    setup: Setup,
    steps: usize,
    sparse: SparseSolverType,
) -> (Histories, Option<SparseSolverType>) {
    let (mut model, mut ms) = build(comm, setup, sparse);
    let kind = ms.dof_manager.backend().sparse_solver_kind("J");
    let mut hist = Vec::new();
    for _ in 0..steps {
        ms.solve_step(&mut model, None).unwrap();
        let nls = ms.non_linear_solver(None).unwrap();
        hist.push((nls.nb_iterations(), nls.error()));
    }
    let dm = &ms.dof_manager;
    let u_g = dm.dofs("disp").unwrap();
    let u = u_g.as_slice();
    let v_g = dm.dofs_derivative("disp", 1).unwrap();
    let v = v_g.as_slice();
    let a_g = dm.dofs_derivative("disp", 2).unwrap();
    let a = a_g.as_slice();
    let state = model
        .global_ids
        .iter()
        .enumerate()
        .map(|(k, &g)| (g, [u[k], v[k], a[k]]))
        .collect();
    ((hist, state), kind)
}

/// Upstream printing of the dynamic tests (`std::scientific`, width 14),
/// as in `test_model_solver.rs`.
fn dynamic_print(energies: &[f64], dt: f64) -> String {
    let w = |s: &str| setw(s, 14);
    let mut out = format!(
        "{},{},{},{},{},{},{}\n",
        w("time"),
        w("wext"),
        w("epot"),
        w("ekin"),
        w("total"),
        w("max_disp"),
        w("min_disp")
    );
    for (i, e) in energies.chunks(6).enumerate() {
        let t = if i == 0 { 0.0 } else { dt * i as f64 };
        let sci = |x: f64| akantu_solver::format::c_scientific(x, 6);
        out += &format!(
            "{},{},{},{},{},{},{}\n",
            w(&sci(t)),
            w(&sci(e[0])),
            w(&sci(e[1])),
            w(&sci(e[2])),
            w(&sci(e[3])),
            w(&sci(e[4])),
            w(&sci(e[5]))
        );
    }
    out
}

/// `test_model_solver_mumps` (static Newton–Raphson, pseudo-time,
/// `max_iterations = 2`, MUMPS): `printResults` equal to
/// `test_model_solver_mumps.verified`.
fn test_model_solver_mumps(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        not_applicable(c, "serial toy mesh (upstream runs it on 1 process)");
        return Ok(());
    }
    let f = case("static_model_solver");
    let mut dm = DofManager::new("model_solver:dof_manager_default", c.clone());
    let mut model =
        MyModel::new(-10.0, gen_mesh(100), false, &mut dm).map_err(|e| e.to_string())?;
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Mumps),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("static", &opts, &mut model)
        .map_err(|e| e.to_string())?;
    ensure!(
        ms.dof_manager.backend().sparse_solver_kind("J") == Some(SparseSolverType::Mumps),
        "installed solver {:?}",
        ms.dof_manager.backend().sparse_solver_kind("J")
    );
    ms.set_integration_scheme(
        "static",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &model,
    )
    .map_err(|e| e.to_string())?;
    ms.non_linear_solver_mut(Some("static"))
        .map_err(|e| e.to_string())?
        .set_param("max_iterations", ParamValue::Int(2))
        .map_err(|e| e.to_string())?;
    ms.solve_step(&mut model, None).map_err(|e| e.to_string())?;
    let g = |x: f64| akantu_solver::format::c_general(x, 7);
    let mut out = format!(
        "node, {}, {}, {}\n",
        setw("disp", 8),
        setw("force", 8),
        setw("blocked", 8)
    );
    let dm = &ms.dof_manager;
    let u_g = dm.dofs("disp").map_err(|e| e.to_string())?;
    let u = u_g.as_slice();
    let b_g = dm.blocked_dofs("disp").map_err(|e| e.to_string())?;
    let b = b_g.as_slice();
    for i in 0..100 {
        out += &format!(
            "{i}, {}, {}, {}\n",
            setw(&g(u[i]), 8),
            setw(&g(model.forces[i]), 8),
            setw(if b[i] { "1" } else { "0" }, 8)
        );
    }
    let eigen = f64s(&f, "displacement");
    let (nd, md) = diff_stats(u, &eigen);
    println!(
        "INFO test_model_solver_mumps: u vs the oracle's Eigen run: {nd} differing entries, max |diff| {md:.3e}"
    );
    ensure!(
        same_text_ignoring_whitespace(&out, &input_text(&f, "verified")),
        "printResults differs from test_model_solver_mumps.verified"
    );
    Ok(())
}

/// `test_model_solver_dynamic_implicit` with MUMPS: printed table equal to
/// its `.verified` file (produced by upstream with MUMPS).
fn test_model_solver_dynamic_implicit_mumps(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        not_applicable(c, "serial toy mesh (upstream runs it on 1 process)");
        return Ok(());
    }
    let f = case("dynamic_implicit");
    let run = run_dynamic_on(&f, SparseSolverType::Mumps, c.clone());
    let printed = dynamic_print(&run.energies, 0.001);
    ensure!(
        same_text_ignoring_whitespace(&printed, &input_text(&f, "verified")),
        "printed table differs from test_model_solver_dynamic_implicit.verified"
    );
    Ok(())
}

/// `test_dof_manager_default` (generic DOFs, `_linear` solver with
/// `SparseSolverType::_mumps` as upstream): printed table equal to the
/// `.verified` file.
fn test_dof_manager_default_mumps(c: &Communicator) -> Outcome {
    struct Cb {
        forces: Vec<f64>,
        nb: usize,
    }
    impl SolverCallback for Cb {
        fn matrix_type(&self, id: &str) -> MatrixType {
            if id == "K" {
                MatrixType::Symmetric
            } else {
                MatrixType::NotDefined
            }
        }
        fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> akantu_core::Result<()> {
            if id != "K" {
                return Ok(());
            }
            let n = self.nb as i64;
            let k = dm.matrix_mut("K")?;
            k.zero();
            for i in 1..n - 1 {
                k.add(i, i, 2.0)?;
            }
            for i in 0..n - 1 {
                k.add(i, i + 1, -1.0)?;
            }
            k.add(0, 0, 1.0)?;
            k.add(n - 1, n - 1, 1.0)?;
            k.mul((n - 1) as f64);
            Ok(())
        }
        fn assemble_lumped_matrix(
            &mut self,
            _: &mut DofManager,
            _: &str,
        ) -> akantu_core::Result<()> {
            Ok(())
        }
        fn assemble_residual(&mut self, dm: &mut DofManager) -> akantu_core::Result<()> {
            let fa = Array::from_vec(self.forces.clone(), 1)?;
            dm.assemble_to_residual("disp", fa.view(), 1.0)
        }
    }
    if c.size() > 1 {
        not_applicable(c, "generic DOFs of one process (upstream: serial test)");
        return Ok(());
    }
    let e2s = |e: akantu_core::Error| e.to_string();
    let f = case("dof_manager_default");
    let nb = 11;
    let mut dm = DofManager::new("test_dof_manager", c.clone());
    dm.register_generic_dofs("disp", nb, 1).map_err(e2s)?;
    dm.register_blocked_dofs("disp").map_err(e2s)?;
    dm.blocked_dofs_mut("disp").map_err(e2s)?.as_mut_slice()[0] = true;
    let mut cb = Cb {
        forces: (0..nb)
            .map(|i| if i == nb - 1 { 10.0 } else { 0.0 })
            .collect(),
        nb,
    };
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::Linear),
        sparse_solver_type: Some(SparseSolverType::Mumps),
        ..ModelSolverOptions::default()
    };
    ms.new_solver("my_tss", &opts).map_err(e2s)?;
    ms.set_integration_scheme(
        "my_tss",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &cb,
    )
    .map_err(e2s)?;
    ms.solve_step(&mut cb, None).map_err(e2s)?;
    let dm = &ms.dof_manager;
    let g = |x: f64| akantu_solver::format::c_general(x, 6);
    let mut out = format!(
        "{} {} {}\n",
        setw("disp", 8),
        setw("force", 8),
        setw("blocked", 8)
    );
    let u_g = dm.dofs("disp").map_err(e2s)?;
    let u = u_g.as_slice();
    let b_g = dm.blocked_dofs("disp").map_err(e2s)?;
    let b = b_g.as_slice();
    for i in 0..nb {
        out += &format!(
            "{} {} {}\n",
            setw(&g(u[i]), 8),
            setw(&g(cb.forces[i]), 8),
            setw(if b[i] { "true" } else { "false" }, 8)
        );
    }
    ensure!(
        same_text_ignoring_whitespace(&out, &input_text(&f, "verified")),
        "printed table differs from test_dof_manager_default.verified"
    );
    Ok(())
}

/// Gather `(global id, [u, u̇, ü])` of every rank on rank 0, sorted by id
/// (shared nodes once: their values are synchronised).
fn gather_state(c: &Communicator, state: &[(i64, [Real; 3])]) -> Vec<(i64, [Real; 3])> {
    let flat: Vec<f64> = state
        .iter()
        .flat_map(|(g, s)| [*g as f64, s[0], s[1], s[2]])
        .collect();
    let all = c.gather_v(&flat, 0).expect("gather");
    let mut out: Vec<(i64, [Real; 3])> = all
        .unwrap_or_default()
        .into_iter()
        .flat_map(|v| {
            v.chunks(4)
                .map(|k| (k[0] as i64, [k[1], k[2], k[3]]))
                .collect::<Vec<_>>()
        })
        .collect();
    out.sort_by_key(|e| e.0);
    out.dedup_by_key(|e| e.0);
    out
}

/// The partitioned bar with `auto` (→ MUMPS on the MPI communicator) vs
/// the serial Eigen run.
fn partitioned_bar_auto_is_mumps(c: &Communicator) -> Outcome {
    let p = c.size();
    let mut fails = Vec::new();
    for (setup, steps) in [
        (Setup::StaticNewton, 1),
        (Setup::StaticLinearSolver, 1),
        (Setup::ImplicitNewmark, 20),
    ] {
        let ((hist, state), kind) = run(c, setup, steps, SparseSolverType::Auto);
        let gathered = gather_state(c, &state);
        if kind != Some(SparseSolverType::Mumps) {
            fails.push(format!("{setup:?}: auto installed {kind:?}"));
        }
        if c.rank() == 0 {
            let serial = Communicator::serial();
            let ((h_ref, s_ref), k_ref) = run(&serial, setup, steps, SparseSolverType::Auto);
            if k_ref != Some(SparseSolverType::Eigen) {
                fails.push(format!("{setup:?}: serial auto installed {k_ref:?}"));
            }
            let iters: Vec<usize> = hist.iter().map(|h| h.0).collect();
            let iters_ref: Vec<usize> = h_ref.iter().map(|h| h.0).collect();
            let mut s_ref = s_ref;
            s_ref.sort_by_key(|e| e.0);
            let scale = s_ref
                .iter()
                .flat_map(|e| e.1)
                .fold(0.0_f64, |m, v| m.nan_max(v.abs()))
                .max(f64::MIN_POSITIVE);
            let mut max_rel = 0.0_f64;
            let mut same = gathered.len() == s_ref.len();
            for (a, b) in gathered.iter().zip(&s_ref) {
                same &= a.0 == b.0;
                for k in 0..3 {
                    max_rel = max_rel.nan_max((a.1[k] - b.1[k]).abs() / scale);
                }
            }
            println!(
                "INFO partitioned bar {setup:?}: ranks={p} MUMPS vs serial Eigen: iterations {iters:?} \
                 vs {iters_ref:?}, max rel diff {max_rel:.3e}"
            );
            if !same || iters != iters_ref || max_rel > 1e-12 {
                fails.push(format!(
                    "{setup:?}: differs from the serial Eigen run ({max_rel:e})"
                ));
            }
        }
    }
    if fails.is_empty() {
        Ok(())
    } else {
        Err(fails.join("; "))
    }
}

/// `DofManager::release_factorization` with MUMPS on the partitioned bar
/// (every rank count): two static solves (the second with a doubled load),
/// `J`'s factorisation (the MUMPS instance, `JOB = -2` agreed on every
/// rank) and the storage of `J` and `K` released between them; the second
/// solve initialises a new instance, assembles, analyses and factorises
/// again and gives the bits of the run that kept everything (same ranks,
/// same input order).
fn release_factorization_mumps(c: &Communicator) -> Outcome {
    /// Displacements by global node and the entries left after the release.
    type Solved = std::result::Result<(Vec<(i64, [Real; 3])>, usize), String>;
    let solve_twice = |release: bool| -> Solved {
        let e2s = |e: akantu_core::Error| e.to_string();
        let (mut model, mut ms) = build(c, Setup::StaticLinearSolver, SparseSolverType::Mumps);
        ms.solve_step(&mut model, None).map_err(e2s)?;
        let mut left = usize::MAX;
        if release {
            ms.dof_manager.release_factorization("J").map_err(e2s)?;
            ms.dof_manager.release_matrix("J").map_err(e2s)?;
            ms.dof_manager.release_matrix("K").map_err(e2s)?;
            model.release_stiffness();
            left = ms.dof_manager.matrix("J").map_err(e2s)?.nnz()
                + ms.dof_manager.matrix("K").map_err(e2s)?.nnz();
        }
        for f in &mut model.forces {
            *f *= 2.0;
        }
        ms.solve_step(&mut model, None).map_err(e2s)?;
        let u_g = ms.dof_manager.dofs("disp").map_err(e2s)?;
        let u = u_g.as_slice();
        let state = model
            .global_ids
            .iter()
            .enumerate()
            .map(|(k, &g)| (g, [u[k], 0.0, 0.0]))
            .collect();
        Ok((state, left))
    };
    let (kept, _) = solve_twice(false)?;
    let (released, left) = solve_twice(true)?;
    ensure!(
        left == 0,
        "{left} entries left in J and K after the release"
    );
    let bits = |v: &[(i64, [Real; 3])]| {
        v.iter()
            .map(|e| (e.0, e.1[0].to_bits()))
            .collect::<Vec<_>>()
    };
    ensure!(
        bits(&kept) == bits(&released),
        "the solve after the release differs from the kept factorisation's"
    );
    ensure!(kept.iter().any(|e| e.1[0] != 0.0), "trivial solution");
    Ok(())
}

/// The release with a profile that grew after the first assembly and with
/// a promoted `K` (`common::release::chain_solves`), MUMPS on every rank
/// count (each rank holds a chain of generic DOFs of its own): after the
/// release the next solve has `K`'s and `J`'s coordinates, values and the
/// solution bit for bit (the same ranks, the same input order).
fn release_grown_promoted_mumps(c: &Communicator) -> Outcome {
    common::release::chain_release_keeps_the_bits(c, SparseSolverType::Mumps)
}

/// The manager-wide UD-216 switch
/// (`UpstreamCompat::mumps_retry_with_solution_rhs`) drives the MUMPS
/// solver of the manager's solves, through the real stack (DOF manager →
/// model solver → linear solver → AIJ backend → MUMPS). System: the
/// `elasticity_6` system of the MUMPS acceptance tests (1029 DOFs, generic
/// DOFs of one process; `ICNTL(14) = −80` forces 8 factorisation retries,
/// `mumps_mpi.rs::ud216_workspace_retry`), assembled by the callback in the
/// same order. A MUMPS solver with that start is installed for `J`; with the
/// switch off the solution equals the plain solve (corrected retry), with
/// the switch on it is upstream's `A^-(k+1) b` — only the manager's switch
/// changes between the two runs. One process (generic DOFs; the agreement
/// of the setting across ranks is `review_v54_regressions.rs::
/// mu02_divergent_retry_rhs`).
fn ud216_manager_compat(c: &Communicator) -> Outcome {
    struct Cb {
        sys: mumps_common::System,
    }
    impl SolverCallback for Cb {
        fn matrix_type(&self, id: &str) -> MatrixType {
            if id == "K" {
                MatrixType::Symmetric
            } else {
                MatrixType::NotDefined
            }
        }
        fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> akantu_core::Result<()> {
            if id != "K" {
                return Ok(());
            }
            let k = dm.matrix_mut("K")?;
            k.zero();
            for &(i, j, v) in &self.sys.per_rank[0] {
                k.add(i, j, v)?;
            }
            Ok(())
        }
        fn assemble_lumped_matrix(
            &mut self,
            _: &mut DofManager,
            _: &str,
        ) -> akantu_core::Result<()> {
            Ok(())
        }
        fn assemble_residual(&mut self, dm: &mut DofManager) -> akantu_core::Result<()> {
            let f = Array::from_vec(self.sys.b.clone(), 1)?;
            dm.assemble_to_residual("disp", f.view(), 1.0)
        }
    }
    if c.size() > 1 {
        not_applicable(c, "generic DOFs of one process");
        return Ok(());
    }
    let e2s = |e: akantu_core::Error| e.to_string();
    let solve_with = |compat: bool, icntl14: Option<i32>| -> Result<Vec<Real>, String> {
        let mut cb = Cb {
            sys: mumps_common::elasticity(6, 1),
        };
        let n = cb.sys.n;
        let mut dm = DofManager::new("ud216_manager_compat", c.clone());
        dm.register_generic_dofs("disp", n, 1).map_err(e2s)?;
        dm.register_blocked_dofs("disp").map_err(e2s)?;
        let mut ms = ModelSolver::new(dm);
        let opts = ModelSolverOptions {
            time_step_solver_type: Some(TimeStepSolverType::Static),
            non_linear_solver_type: Some(NonLinearSolverType::Linear),
            sparse_solver_type: Some(SparseSolverType::Mumps),
            ..ModelSolverOptions::default()
        };
        ms.new_solver("s", &opts).map_err(e2s)?;
        ms.set_integration_scheme(
            "s",
            "disp",
            IntegrationSchemeType::PseudoTime,
            SolutionType::NotDefined,
            &cb,
        )
        .map_err(e2s)?;
        let mut mumps = MumpsOptions::default();
        if let Some(v) = icntl14 {
            mumps.icntl.insert(14, v);
        }
        ms.dof_manager
            .backend_mut()
            .map_err(e2s)?
            .set_sparse_solver("J", Box::new(MumpsSolver::with_options(mumps)))
            .map_err(e2s)?;
        ms.dof_manager.set_upstream_compat(UpstreamCompat {
            mumps_retry_with_solution_rhs: compat,
            ..UpstreamCompat::default()
        });
        ms.solve_step(&mut cb, None).map_err(e2s)?;
        Ok(ms
            .dof_manager
            .dofs("disp")
            .map_err(e2s)?
            .as_slice()
            .to_vec())
    };
    let plain = solve_with(false, None)?;
    let corrected = solve_with(false, Some(-80))?;
    let upstream = solve_with(true, Some(-80))?;
    let rel = |a: &[Real], b: &[Real]| {
        let scale = b
            .iter()
            .fold(0.0_f64, |m, v| m.nan_max(v.abs()))
            .max(f64::MIN_POSITIVE);
        a.iter()
            .zip(b)
            .fold(0.0_f64, |m, (x, y)| m.nan_max((x - y).abs()))
            / scale
    };
    let (d_corr, d_up) = (rel(&corrected, &plain), rel(&upstream, &plain));
    let bitwise = corrected.len() == plain.len()
        && corrected
            .iter()
            .zip(&plain)
            .all(|(x, y)| x.to_bits() == y.to_bits());
    println!(
        "INFO ud216_manager_compat: corrected retry vs plain {d_corr:.3e} (bitwise {bitwise}), \
         manager compat (upstream retry) vs plain {d_up:.3e}"
    );
    ensure!(
        corrected.len() == plain.len() && d_corr <= 1e-12,
        "corrected retry differs from the plain solve by {d_corr:e}"
    );
    ensure!(
        d_up > 1e-3,
        "the manager-wide UD-216 switch did not reach the MUMPS solver (difference {d_up:e})"
    );
    Ok(())
}

fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(900));
        eprintln!("mumps_mpi_solver: watchdog expired");
        std::process::exit(70);
    });
    let control = world();
    let work = world();
    let rank = control.rank();
    // `lr20_dbltrace_rank_files` first: upstream's dblTrace statics (the
    // first-call flag and the dump counter) are process-wide and fresh here
    let scenarios: [Scenario; 8] = [
        (
            "lr20_dbltrace_rank_files",
            dbltrace_mpi::lr20_dbltrace_rank_files,
        ),
        ("test_model_solver_mumps", test_model_solver_mumps),
        (
            "test_model_solver_dynamic_implicit_mumps",
            test_model_solver_dynamic_implicit_mumps,
        ),
        (
            "test_dof_manager_default_mumps",
            test_dof_manager_default_mumps,
        ),
        (
            "partitioned_bar_auto_is_mumps",
            partitioned_bar_auto_is_mumps,
        ),
        ("ud216_manager_compat", ud216_manager_compat),
        ("release_factorization_mumps", release_factorization_mumps),
        ("release_grown_promoted_mumps", release_grown_promoted_mumps),
    ];
    let mut failed = 0;
    for (name, f) in scenarios {
        let t0 = Instant::now();
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&work)))
            .unwrap_or_else(|_| Err("panicked".into()));
        let mut ok = [u8::from(out.is_ok())];
        control.all_reduce(&mut ok, ReduceOp::Min).expect("control");
        if ok[0] == 0 {
            failed += 1;
        }
        if let Err(e) = &out {
            println!("rank {rank}: {name}: {e}");
        }
        if rank == 0 {
            let v = if ok[0] == 1 { "PASS" } else { "FAIL" };
            println!("{v} {name} ({:.2} s)", t0.elapsed().as_secs_f64());
        }
    }
    if rank == 0 {
        println!("mumps_mpi_solver: failed={failed}");
    }
    drop(work);
    drop(control);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
