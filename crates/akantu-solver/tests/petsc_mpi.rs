// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_solver/test_sparse_solver_petsc.cc
// (TEST-160), test/test_solver/test_petsc_matrix_{profile,profile_parallel,
// diagonal,apply_boundary}.cc and test_solver_petsc.cc (TEST-228, intents:
// profile, diagonal, Dirichlet rows, KSP solve with options),
// test/test_model/test_common/test_model_solver/{test_model_solver.cc
// (test_model_solver_petsc, TEST-073), test_model_solver_dynamic.cc
// (test_model_solver_dynamic_petsc, TEST-074)} with their .verified outputs,
// and the partitioned MyModel toy (test_model_solver_my_model.hh).

//! Real-MPI acceptance of the PETSc backend, `harness = false`:
//! run under `mpirun -np 1/2/4` by `scripts/petsc/run-petsc-tests.sh`
//! (features `petsc`; image `akantu-rs-petsc:dev`). Scenarios print one
//! `PASS`/`FAIL` line on rank 0 and `INFO` lines with the measured values;
//! the tolerance rules are `docs/NUMERICAL_VALIDATION.md`.
//!
//! `AKANTU_PETSC_THREAD_LEVEL=serialized` runs the whole binary under
//! `MPI_THREAD_SERIALIZED` (native regions with the re-entrant lock and the
//! split-class admission).

mod common;

use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, Synchronizer};
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Array, Error, GhostType, Idx, NodeFlag, Real};
use akantu_linalg::TermsToAssemble;
use akantu_linalg::petsc;
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::param::{ParamValue, ParameterRegistry};
use akantu_solver::petsc_backend::PetscBackend;
use akantu_solver::{
    DofManager, LinearSystemBackend, ModelSolver, ModelSolverOptions, SolutionType,
};

use akantu_validation::fixture::{oracle_report_line, oracle_values_not_comparable};
use common::drivers::run_dynamic_with;
use common::my_model::{Law, MyModel, SEG, gen_mesh};
use common::oracle::{case, same_text_ignoring_whitespace, setw};

type Outcome = std::result::Result<(), String>;

macro_rules! ensure {
    ($c:expr, $($fmt:tt)+) => {
        let ok: bool = $c;
        if !ok {
            return Err(format!($($fmt)+));
        }
    };
}

// Isolated regression cases of the fix round, selected with
// AKANTU_PETSC_CASE.
mod petsc_mpi_cases;

// PETSc / TAO model scenarios.
mod petsc_pftao;

// Regressions of the safe PETSc wrappers (the AO sweep).
mod petsc_unsafe_fix;

fn e2s(e: Error) -> String {
    e.to_string()
}

/// A named scenario of the binary.
type Scenario = (&'static str, fn(&Communicator) -> Outcome);

const N: usize = 17; // 16 elements: 16, 8, 4 per rank at 1, 2, 4 ranks

fn thread_level() -> ThreadLevel {
    match std::env::var("AKANTU_PETSC_THREAD_LEVEL").as_deref() {
        Ok("serialized") => ThreadLevel::Serialized,
        Ok("multiple") => ThreadLevel::Multiple,
        _ => ThreadLevel::Funneled,
    }
}

fn world() -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: thread_level(),
        timeout: Some(Duration::from_secs(600)),
        ..MpiOptions::default()
    })
    .expect("MPI world")
}

fn petsc_dm(id: &str, c: &Communicator) -> std::result::Result<DofManager, String> {
    let backend = PetscBackend::new(c).map_err(e2s)?;
    Ok(DofManager::with_backend(id, c.clone(), Box::new(backend)))
}

/// Local equation of DOF `k` of family `fam` (one component).
fn eq(dm: &DofManager, fam: &str, k: usize) -> std::result::Result<usize, String> {
    let l = dm.local_equation_numbers(fam).map_err(e2s)?;
    l.get(k)
        .map(|&e| e as usize)
        .ok_or_else(|| format!("no DOF {k} in {fam}"))
}

fn petsc_backend(dm: &mut DofManager) -> std::result::Result<&mut PetscBackend, String> {
    dm.backend_mut()
        .map_err(e2s)?
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<PetscBackend>())
        .ok_or_else(|| "not a PETSc backend".to_owned())
}

/// Rank `r` of `p` holds global nodes `[r·e, (r+1)·e]` (`e = (N−1)/p`); the
/// boundary node is master on the lower rank and slave on the upper one.
fn partitioned_bar(comm: &Communicator, n: usize) -> (Mesh, Vec<i64>) {
    let (r, p) = (comm.rank() as usize, comm.size());
    let e = (n - 1) / p;
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
            let node = acc.push_node(&[g as f64 * (1.0 / (n - 1) as f64)]).unwrap();
            gids.push(g as i64);
            let flag = if p > 1 && i == 0 && r > 0 {
                NodeFlag::SLAVE
            } else if p > 1 && i == e && r + 1 < p {
                NodeFlag::MASTER
            } else {
                NodeFlag::NORMAL
            };
            acc.nodes_flags_mut().as_mut_slice()[node as usize] = flag;
        }
        {
            let mut c = acc.connectivity_mut(SEG, GhostType::NotGhost);
            for i in 0..e {
                c.push_tuple(&[i as Idx, i as Idx + 1]);
            }
        }
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

/// Gather `(global id, values)` of every rank on rank 0, sorted by id
/// (shared nodes once: their values are synchronised).
fn gather_by_id(c: &Communicator, ids: &[i64], v: &[Real]) -> Vec<(i64, Real)> {
    let flat: Vec<f64> = ids
        .iter()
        .zip(v)
        .flat_map(|(g, x)| [*g as f64, *x])
        .collect();
    let all = c.gather_v(&flat, 0).expect("gather");
    let mut out: Vec<(i64, Real)> = all
        .unwrap_or_default()
        .into_iter()
        .flat_map(|v| v.chunks(2).map(|k| (k[0] as i64, k[1])).collect::<Vec<_>>())
        .collect();
    out.sort_by_key(|e| e.0);
    out.dedup_by_key(|e| e.0);
    out
}

fn max_rel(a: &[(i64, Real)], b: &[(i64, Real)]) -> Option<f64> {
    if a.len() != b.len() || a.iter().zip(b).any(|(x, y)| x.0 != y.0) {
        return None;
    }
    let scale = b
        .iter()
        .fold(0.0_f64, |m, x| m.max(x.1.abs()))
        .max(f64::MIN_POSITIVE);
    Some(
        a.iter()
            .zip(b)
            .fold(0.0_f64, |m, (x, y)| m.max((x.1 - y.1).abs() / scale)),
    )
}

// ------------------------------------------------------------- TEST-160

/// `test_sparse_solver_petsc`: diagonal system `A_ii = 1/(1 + g_i)`
/// assembled from pre-assembled terms on local-or-master DOFs, right-hand
/// side 1, KSP solve; `x_i = 1 + g_i` to upstream's `1e-15` relative. The
/// upstream test throws on more than one process ("need to fix synchronizer
/// for petsc case", unfinished); the port checks the gathered solution on
/// every rank count.
fn test_sparse_solver_petsc(c: &Communicator) -> Outcome {
    let (mesh, gids) = partitioned_bar(c, mesh_global_nodes(c));
    let mut dm = petsc_dm("test_dof_manager", c)?;
    dm.register_nodal_dofs(&mesh, "x", 1).map_err(e2s)?;
    dm.new_matrix("A", MatrixType::Symmetric).map_err(e2s)?;
    let mut terms = TermsToAssemble::new("x", "x");
    for (i, _) in gids.iter().enumerate() {
        if dm.is_local_or_master_dof(i).map_err(e2s)? {
            let li = eq(&dm, "x", i)?;
            let gi = dm.local_to_global_equation_number(li).ok_or("pure ghost")?;
            terms.add(i as Idx, i as Idx, 1.0 / (1.0 + gi as f64));
        }
    }
    dm.assemble_preassembled_matrix("A", &terms).map_err(e2s)?;
    dm.finalize_matrix("A").map_err(e2s)?;
    let b = Array::from_vec(vec![1.0; mesh.nb_nodes()], 1).map_err(e2s)?;
    dm.zero_residual();
    dm.assemble_to_residual("x", b.view(), 1.0).map_err(e2s)?;
    dm.solve("A").map_err(e2s)?;
    let y = dm.dofs_solution("x").map_err(e2s)?.as_slice().to_vec();
    // Global equation of each local node (single family, one component).
    let geq: Vec<i64> = (0..mesh.nb_nodes())
        .map(|i| {
            let l = eq(&dm, "x", i).unwrap();
            dm.local_to_global_equation_number(l).unwrap()
        })
        .collect();
    let gathered = gather_by_id(c, &geq, &y);
    let ksp = petsc_backend(&mut dm)?.last_ksp_outcome("A").cloned();
    if c.rank() == 0 {
        println!(
            "INFO test_sparse_solver_petsc: ranks={} ksp={ksp:?}",
            c.size()
        );
        for (g, x) in &gathered {
            let d = 1.0 + *g as f64;
            ensure!(
                (x - d).abs() / d <= 1e-15,
                "Error in the solution: {x} != {d} [{}]",
                (x - d).abs() / d
            );
        }
        ensure!(
            gathered.len() == mesh_global_nodes(c),
            "gathered {} values",
            gathered.len()
        );
    }
    Ok(())
}

/// Nodes of `test_sparse_solver_petsc`'s bar: upstream's 11 on one or two
/// ranks, otherwise the smallest `n ≥ 13` whose `n − 1` elements split
/// evenly over the ranks (13 at 3/4 ranks, 17 at 8; the
/// 13-node bar was not 8-rank capable).
fn mesh_global_nodes(c: &Communicator) -> usize {
    let p = c.size();
    if p <= 2 {
        11
    } else {
        (13..).find(|n| (n - 1) % p == 0).unwrap_or(13)
    }
}

// ------------------------------------------------------------- TEST-073

/// `test_model_solver_petsc` (static Newton–Raphson, pseudo-time,
/// `max_iterations = 2`, PETSc DOF manager with the default KSP):
/// `printResults` equal to `test_model_solver_petsc.verified`.
fn test_model_solver_petsc(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        return Ok(()); // serial toy mesh (upstream runs it on 1 process)
    }
    let mut dm = petsc_dm("model_solver:dof_manager_petsc", c)?;
    dm.set_upstream_compat(common::drivers::upstream_compat());
    let mut model = MyModel::new(-10.0, gen_mesh(100), false, &mut dm).map_err(e2s)?;
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("static", &opts, &mut model)
        .map_err(e2s)?;
    ensure!(
        ms.dof_manager.backend().sparse_solver_kind("J") == Some(SparseSolverType::Petsc),
        "auto did not resolve to petsc"
    );
    ms.set_integration_scheme(
        "static",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &model,
    )
    .map_err(e2s)?;
    ms.non_linear_solver_mut(Some("static"))
        .map_err(e2s)?
        .set_param("max_iterations", ParamValue::Int(2))
        .map_err(e2s)?;
    let r = ms.solve_step(&mut model, None);
    let nls = ms.non_linear_solver(None).map_err(e2s)?;
    println!(
        "INFO test_model_solver_petsc: solve {r:?}, NR iterations {}, error {:e}, ksp {:?}",
        nls.nb_iterations(),
        nls.error(),
        petsc_backend(&mut ms.dof_manager)?.last_ksp_outcome("J")
    );
    let g = |x: f64| akantu_solver::format::c_general(x, 7);
    let mut out = format!(
        "node, {}, {}, {}\n",
        setw("disp", 8),
        setw("force", 8),
        setw("blocked", 8)
    );
    let dm = &ms.dof_manager;
    let u_g = dm.dofs("disp").map_err(e2s)?;
    let u = u_g.as_slice();
    let b_g = dm.blocked_dofs("disp").map_err(e2s)?;
    let b = b_g.as_slice();
    for i in 0..100 {
        out += &format!(
            "{i}, {}, {}, {}\n",
            setw(&g(u[i]), 8),
            setw(&g(model.forces[i]), 8),
            setw(if b[i] { "1" } else { "0" }, 8)
        );
    }
    let verified =
        include_str!("../../../fixtures/solver/petsc/upstream/test_model_solver_petsc.verified");
    if !same_text_ignoring_whitespace(&out, verified) {
        let first = out
            .lines()
            .zip(verified.lines())
            .find(|(a, b)| a.split_whitespace().ne(b.split_whitespace()));
        return Err(format!(
            "printResults differs from the .verified file: {first:?}"
        ));
    }
    Ok(())
}

// ------------------------------------------------------------- TEST-074

/// Upstream printing of the dynamic tests (`std::scientific`, width 14).
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

/// `test_model_solver_dynamic_petsc` (implicit trapezoidal rule, 400 steps,
/// PETSc DOF manager, default KSP): printed table equal to
/// `test_model_solver_dynamic_petsc.verified`.
fn test_model_solver_dynamic_petsc(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        return Ok(());
    }
    let f = case("dynamic_implicit");
    let dm = petsc_dm("model_solver:dof_manager_petsc", c)?;
    let mut run = run_dynamic_with(&f, SparseSolverType::Petsc, dm);
    if let Some((step, e)) = &run.failure {
        return Err(format!("step {step}: {e}"));
    }
    let printed = dynamic_print(&run.energies, 0.001);
    let ksp = run
        .solver
        .as_mut()
        .and_then(|ms| petsc_backend(&mut ms.dof_manager).ok())
        .and_then(|b| b.last_ksp_outcome("J").cloned());
    println!(
        "INFO test_model_solver_dynamic_petsc: NR iterations per step (first 10) {:?}, last ksp {ksp:?}",
        &run.n_iter[..run.n_iter.len().min(10)]
    );
    let verified = include_str!(
        "../../../fixtures/solver/petsc/upstream/test_model_solver_dynamic_petsc.verified"
    );
    if !same_text_ignoring_whitespace(&printed, verified) {
        let first = printed
            .lines()
            .zip(verified.lines())
            .enumerate()
            .find(|(_, (a, b))| a.split_whitespace().ne(b.split_whitespace()));
        return Err(format!(
            "printed table differs from the .verified file: {first:?}"
        ));
    }
    Ok(())
}

// ------------------------------------------------- partitioned bar runs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setup {
    StaticNewton,
    StaticLinear,
    StaticCubicNewton,
    ImplicitNewmark,
    SnesLinear,
    SnesCubic,
    TaoLinear,
}

/// KSP configuration of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KspConf {
    /// Upstream default (no option set): GMRES + ILU(0) / block Jacobi.
    Default,
    /// Direct: `preonly` + LU (MUMPS through PETSc on several ranks).
    Direct,
}

fn ksp_options(conf: KspConf, p: usize) -> Vec<(&'static str, &'static str)> {
    match conf {
        KspConf::Default => Vec::new(),
        KspConf::Direct => {
            let mut o = vec![("ksp_type", "preonly"), ("pc_type", "lu")];
            if p > 1 {
                o.push(("pc_factor_mat_solver_type", "mumps"));
            }
            o
        }
    }
}

/// Build the partitioned toy on `comm` with the PETSc backend (`petsc`) or
/// the default backend with Eigen (`!petsc`, serial reference).
fn build(
    comm: &Communicator,
    setup: Setup,
    petsc: bool,
    conf: KspConf,
) -> std::result::Result<(MyModel, ModelSolver), String> {
    let (mesh, gids) = partitioned_bar(comm, N);
    let x: Vec<f64> = mesh.nodes().as_slice().to_vec();
    let mut dm = if petsc {
        petsc_dm("dm", comm)?
    } else {
        DofManager::new("dm", comm.clone())
    };
    let force = match setup {
        Setup::StaticCubicNewton | Setup::SnesCubic => -2.0,
        Setup::ImplicitNewmark => 0.0,
        _ => -10.0,
    };
    let mut model = MyModel::with_global_ids(force, mesh, false, &mut dm, gids.clone(), N as i64)
        .map_err(e2s)?;
    if matches!(setup, Setup::StaticCubicNewton | Setup::SnesCubic) {
        model.law = Law::Cubic;
    }
    if setup == Setup::ImplicitNewmark {
        let mut b_g = dm.blocked_dofs_mut("disp").map_err(e2s)?;
        let b = b_g.as_mut_slice();
        for (k, g) in gids.iter().enumerate() {
            b[k] = *g == 0 || *g == N as i64 - 1;
        }
        drop(b_g);
        let k = 2.0 * std::f64::consts::PI / 0.2;
        let u: Vec<f64> = x.iter().map(|&xi| 0.01 * (k * xi).cos()).collect();
        let v: Vec<f64> = x.iter().map(|&xi| k * 0.01 * (k * xi).sin()).collect();
        dm.dofs_mut("disp")
            .map_err(e2s)?
            .as_mut_slice()
            .copy_from_slice(&u);
        dm.dofs_derivative_mut("disp", 1)
            .map_err(e2s)?
            .as_mut_slice()
            .copy_from_slice(&v);
    }
    let mut ms = ModelSolver::new(dm);
    let (tss, nls) = match setup {
        Setup::StaticNewton | Setup::StaticCubicNewton => (
            TimeStepSolverType::Static,
            NonLinearSolverType::NewtonRaphson,
        ),
        Setup::StaticLinear => (TimeStepSolverType::Static, NonLinearSolverType::Linear),
        Setup::ImplicitNewmark => (
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
        ),
        Setup::SnesLinear | Setup::SnesCubic => {
            (TimeStepSolverType::Static, NonLinearSolverType::PetscSnes)
        }
        Setup::TaoLinear => (TimeStepSolverType::Static, NonLinearSolverType::PetscTao),
    };
    let native = matches!(
        setup,
        Setup::SnesLinear | Setup::SnesCubic | Setup::TaoLinear
    );
    let (nls, sparse) = if !petsc && native {
        // Serial reference of a native PETSc solve: Newton–Raphson + Eigen.
        (NonLinearSolverType::NewtonRaphson, SparseSolverType::Eigen)
    } else if petsc {
        (nls, SparseSolverType::Petsc)
    } else {
        (nls, SparseSolverType::Eigen)
    };
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(tss),
        non_linear_solver_type: Some(nls),
        sparse_solver_type: Some(sparse),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &opts, &mut model).map_err(e2s)?;
    let (scheme, st) = match setup {
        Setup::ImplicitNewmark => (
            IntegrationSchemeType::TrapezoidalRule2,
            SolutionType::Displacement,
        ),
        _ => (IntegrationSchemeType::PseudoTime, SolutionType::NotDefined),
    };
    ms.set_integration_scheme("s", "disp", scheme, st, &model)
        .map_err(e2s)?;
    ms.set_time_step(0.001, None).map_err(e2s)?;
    let p = comm.size();
    match nls {
        NonLinearSolverType::NewtonRaphson => {
            let nr = ms
                .non_linear_solver_mut(None)
                .map_err(e2s)?
                .as_newton_raphson_mut()
                .ok_or("not a Newton-Raphson solver")?;
            nr.set_param("max_iterations", ParamValue::Int(40))
                .map_err(e2s)?;
        }
        NonLinearSolverType::PetscSnes | NonLinearSolverType::PetscTao => {
            let nls = ms.non_linear_solver_mut(None).map_err(e2s)?;
            for (k, v) in ksp_options(conf, p) {
                nls.set_param(k, ParamValue::String(v.into()))
                    .map_err(e2s)?;
            }
            if nls.kind() == NonLinearSolverType::PetscSnes {
                nls.set_param("snes_rtol", ParamValue::Real(1e-12))
                    .map_err(e2s)?;
                nls.set_param("snes_atol", ParamValue::Real(1e-14))
                    .map_err(e2s)?;
                nls.set_param("snes_stol", ParamValue::Real(0.0))
                    .map_err(e2s)?;
                nls.set_param("snes_max_it", ParamValue::Int(50))
                    .map_err(e2s)?;
            } else {
                nls.set_param("tao_gatol", ParamValue::Real(1e-12))
                    .map_err(e2s)?;
                nls.set_param("tao_grtol", ParamValue::Real(0.0))
                    .map_err(e2s)?;
            }
        }
        _ => {}
    }
    if std::env::var_os("AKANTU_PETSC_DEBUG").is_some() && petsc && !native {
        if let Some(nr) = ms
            .non_linear_solver_mut(None)
            .map_err(e2s)?
            .as_newton_raphson_mut()
        {
            nr.set_param("verbose_iteration", ParamValue::Bool(true))
                .map_err(e2s)?;
        }
        ms.dof_manager
            .backend_mut()
            .map_err(e2s)?
            .set_solver_option("J", "ksp_monitor_true_residual", "")
            .map_err(e2s)?;
    }
    if petsc && !native {
        // KSP options of the Newton–Raphson path (upstream
        // `getSparseSolver().set(...)`).
        for (k, v) in ksp_options(conf, p) {
            ms.dof_manager
                .backend_mut()
                .map_err(e2s)?
                .set_solver_option("J", k, v)
                .map_err(e2s)?;
        }
    }
    Ok((model, ms))
}

/// `(iterations, error)` per step and the final `u` keyed by global id.
type Run = (Vec<(usize, Real)>, Vec<(i64, Real)>);

fn run(
    comm: &Communicator,
    setup: Setup,
    steps: usize,
    petsc: bool,
    conf: KspConf,
) -> std::result::Result<Run, String> {
    let (mut model, mut ms) = build(comm, setup, petsc, conf)?;
    let mut hist = Vec::new();
    for _ in 0..steps {
        ms.solve_step(&mut model, None).map_err(e2s)?;
        let nls = ms.non_linear_solver(None).map_err(e2s)?;
        hist.push((nls.nb_iterations(), nls.error()));
    }
    let u = ms
        .dof_manager
        .dofs("disp")
        .map_err(e2s)?
        .as_slice()
        .to_vec();
    Ok((hist, gather_by_id(comm, &model.global_ids, &u)))
}

/// Serial Eigen reference on rank 0 (`None` elsewhere).
fn reference(
    c: &Communicator,
    setup: Setup,
    steps: usize,
) -> std::result::Result<Option<Run>, String> {
    if c.rank() != 0 {
        return Ok(None);
    }
    run(
        &Communicator::serial(),
        setup,
        steps,
        false,
        KspConf::Default,
    )
    .map(Some)
}

/// Partitioned bar, Newton–Raphson / linear solver / implicit Newmark with
/// the PETSc backend's KSP. Direct KSP (`preonly` + LU): iteration counts
/// equal to the serial Eigen run and `u` within `1e-12` of it (rule
/// PETSC-DIRECT); default KSP (GMRES rtol 1e-5): converged Newton runs
/// within `1e-8` (rule PETSC-KSP-NEWTON).
fn partitioned_bar_newton(c: &Communicator) -> Outcome {
    let p = c.size();
    let mut fails = Vec::new();
    for (setup, steps) in [
        (Setup::StaticNewton, 1),
        (Setup::StaticLinear, 1),
        (Setup::StaticCubicNewton, 1),
        (Setup::ImplicitNewmark, 20),
    ] {
        let reference = reference(c, setup, steps)?;
        for conf in [KspConf::Direct, KspConf::Default] {
            let (hist, state) = match run(c, setup, steps, true, conf) {
                Ok(r) => r,
                Err(e) => {
                    fails.push(format!("{setup:?}/{conf:?}: {e}"));
                    continue;
                }
            };
            if let Some((h_ref, s_ref)) = &reference {
                let iters: Vec<usize> = hist.iter().map(|h| h.0).collect();
                let iters_ref: Vec<usize> = h_ref.iter().map(|h| h.0).collect();
                let rel = max_rel(&state, s_ref);
                println!(
                    "INFO partitioned bar {setup:?} {conf:?}: ranks={p} PETSc vs serial Eigen: \
                     iterations {iters:?} vs {iters_ref:?}, max rel diff {rel:?}"
                );
                let ok = match (conf, setup) {
                    // A linear solve stops after one solve: the KSP
                    // tolerance is the error.
                    (KspConf::Default, Setup::StaticLinear) => rel.is_some_and(|r| r <= 1e-4),
                    (KspConf::Default, _) => rel.is_some_and(|r| r <= 1e-8),
                    (KspConf::Direct, _) => iters == iters_ref && rel.is_some_and(|r| r <= 1e-12),
                };
                if !ok {
                    fails.push(format!("{setup:?}/{conf:?}: differs from Eigen ({rel:?})"));
                }
            }
        }
    }
    if fails.is_empty() {
        Ok(())
    } else {
        Err(fails.join("; "))
    }
}

/// Partitioned bar with the native SNES (linear and cubic laws) and TAO
/// (linear law, a quadratic problem), direct inner KSP, vs the serial Eigen
/// Newton–Raphson solution: `u` within `1e-10` relative (rule PETSC-SNES).
fn partitioned_bar_native(c: &Communicator) -> Outcome {
    let p = c.size();
    let mut fails = Vec::new();
    for setup in [Setup::SnesLinear, Setup::SnesCubic, Setup::TaoLinear] {
        let reference = reference(c, setup, 1)?;
        let (hist, state) = match run(c, setup, 1, true, KspConf::Direct) {
            Ok(r) => r,
            Err(e) => {
                fails.push(format!("{setup:?}: {e}"));
                continue;
            }
        };
        if let Some((h_ref, s_ref)) = &reference {
            let rel = max_rel(&state, s_ref);
            println!(
                "INFO partitioned bar {setup:?}: ranks={p} native iterations {:?} (Eigen NR {:?}), \
                 max rel diff {rel:?}",
                hist.iter().map(|h| h.0).collect::<Vec<_>>(),
                h_ref.iter().map(|h| h.0).collect::<Vec<_>>()
            );
            if !rel.is_some_and(|r| r <= 1e-10) {
                fails.push(format!("{setup:?}: differs from Eigen ({rel:?})"));
            }
        }
    }
    if fails.is_empty() {
        Ok(())
    } else {
        Err(fails.join("; "))
    }
}

// ------------------------------------------------------------- TEST-228

/// Intents of `test_petsc_matrix_{profile,diagonal,apply_boundary}` and the
/// UD-006 fix: two DOF families (application ordering ≠ identity on several
/// ranks), a known matrix assembled from elemental blocks, the PETSc matrix
/// after a solve equal to the gathered assembly (owned rows), Dirichlet rows
/// and columns zeroed with a unit diagonal, and the KSP solution equal to
/// the one of the default backend with Eigen on one process.
fn petsc_matrix_two_families(c: &Communicator) -> Outcome {
    let (mesh, gids) = partitioned_bar(c, N);
    let mut dm = petsc_dm("dm", c)?;
    dm.register_nodal_dofs(&mesh, "u", 1).map_err(e2s)?;
    dm.register_nodal_dofs(&mesh, "v", 1).map_err(e2s)?;
    dm.register_blocked_dofs("u").map_err(e2s)?;
    dm.register_blocked_dofs("v").map_err(e2s)?;
    for (k, g) in gids.iter().enumerate() {
        if *g == 0 {
            dm.blocked_dofs_mut("u").map_err(e2s)?.as_mut_slice()[k] = true;
            dm.blocked_dofs_mut("v").map_err(e2s)?.as_mut_slice()[k] = true;
        }
    }
    dm.new_matrix("K", MatrixType::Symmetric).map_err(e2s)?;
    let nb_el = mesh.nb_element(SEG, GhostType::NotGhost);
    let mut ku = Array::new(4);
    let mut kv = Array::new(4);
    for _ in 0..nb_el {
        ku.push_tuple(&[2.0, -2.0, -2.0, 2.0]);
        kv.push_tuple(&[3.0, -3.0, -3.0, 3.0]);
    }
    for (fam, k) in [("u", &ku), ("v", &kv)] {
        dm.assemble_elemental_matrices_to_matrix(
            &mesh,
            "K",
            fam,
            k.view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
        .map_err(e2s)?;
    }
    dm.finalize_matrix("K").map_err(e2s)?;
    dm.update_global_blocked_dofs();
    dm.apply_boundary("K").map_err(e2s)?;
    let n_local = dm.local_system_size();
    let mut rhs = vec![0.0; n_local];
    for (k, g) in gids.iter().enumerate() {
        if *g == N as i64 - 1 {
            rhs[eq(&dm, "u", k)?] = 1.0;
            rhs[eq(&dm, "v", k)?] = 2.0;
        }
    }
    let mut x = vec![0.0; n_local];
    for (k, v) in ksp_options(KspConf::Direct, c.size()) {
        petsc_backend(&mut dm)?
            .set_solver_option("K", k, v)
            .map_err(e2s)?;
    }
    dm.solve_linear_system("K", &rhs, &mut x).map_err(e2s)?;
    // Exact solution: u = x_coord / 2 · (N-1)... per family: k_e = c/L with
    // L = 1/(N−1): u_g = g / (c (N−1)) · load.
    let mut fails = Vec::new();
    for (k, g) in gids.iter().enumerate() {
        let lu = eq(&dm, "u", k)?;
        let lv = eq(&dm, "v", k)?;
        let eu = *g as f64 / 2.0;
        let ev = *g as f64 * 2.0 / 3.0;
        if (x[lu] - eu).abs() > 1e-12 * eu.abs().max(1.0)
            || (x[lv] - ev).abs() > 1e-12 * ev.abs().max(1.0)
        {
            fails.push(format!("node {g}: ({}, {}) vs ({eu}, {ev})", x[lu], x[lv]));
        }
    }
    // The PETSc matrix: owned rows of node g (both families).
    let layout_rows: Vec<(usize, i64, i64)> = gids
        .iter()
        .enumerate()
        .filter_map(|(k, g)| {
            let lu = eq(&dm, "u", k).ok()?;
            dm.is_local_or_master_dof(lu).ok()?.then_some((k, *g, 0))
        })
        .collect();
    let backend_numbering = petsc_backend(&mut dm)?
        .petsc_numbering()
        .map(<[i32]>::to_vec);
    if c.rank() == 0 {
        println!(
            "INFO petsc_matrix_two_families: ranks={} PETSc numbering of rank 0 {backend_numbering:?}",
            c.size()
        );
    }
    for (k, g, _) in layout_rows {
        for fam in ["u", "v"] {
            let l = eq(&dm, fam, k)?;
            let gr = dm.local_to_global_equation_number(l).ok_or("ghost")?;
            let mut cols = vec![gr];
            for (k2, g2) in gids.iter().enumerate() {
                if (g2 - g).abs() == 1 {
                    let l2 = eq(&dm, fam, k2)?;
                    cols.push(dm.local_to_global_equation_number(l2).ok_or("ghost")?);
                }
            }
            let layout = dm.layout().clone();
            let vals = petsc_backend(&mut dm)?
                .native_values("K", &layout, &[gr], &cols)
                .map_err(e2s)?;
            let c_el = if fam == "u" { 2.0 } else { 3.0 };
            let interior = g != 0 && g != N as i64 - 1;
            let expect_diag = if g == 0 {
                1.0 // Dirichlet: unit diagonal on the owner
            } else if interior {
                2.0 * c_el
            } else {
                c_el
            };
            if vals[0] != expect_diag {
                fails.push(format!(
                    "{fam} node {g}: diagonal {} vs {expect_diag}",
                    vals[0]
                ));
            }
            // Columns of the neighbours: zero when the row or the column is
            // the blocked node 0 (Dirichlet rows and columns zeroed).
            for ((cc, v), g2) in cols
                .iter()
                .skip(1)
                .zip(vals.iter().skip(1))
                .zip(neighbours(&gids, g))
            {
                let blocked_pair = g == 0 || g2 == 0;
                let expect = if blocked_pair { 0.0 } else { -c_el };
                if *v != expect {
                    fails.push(format!("{fam} node {g} col {cc}: {v} vs {expect}"));
                }
            }
        }
    }
    let mut ok = [u8::from(fails.is_empty())];
    c.all_reduce(&mut ok, ReduceOp::Min).map_err(e2s)?;
    if !fails.is_empty() {
        return Err(fails.join("; "));
    }
    ensure!(ok[0] == 1, "failed on another rank");
    Ok(())
}

/// Global ids of the neighbours of node `g` present on this rank, in local
/// order (the column order of `petsc_matrix_two_families`).
fn neighbours(gids: &[i64], g: i64) -> Vec<i64> {
    gids.iter()
        .copied()
        .filter(|g2| (g2 - g).abs() == 1)
        .collect()
}

/// A profile change between two solves (a new coupling term) takes the
/// `MatResetPreallocation` path; the second solve sees the new matrix.
fn petsc_profile_change(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        return Ok(()); // the toy coupling uses local DOFs of one process
    }
    let mut dm = petsc_dm("dm", c)?;
    dm.register_generic_dofs("x", 3, 1).map_err(e2s)?;
    dm.new_matrix("A", MatrixType::Unsymmetric).map_err(e2s)?;
    let mut t = TermsToAssemble::new("x", "x");
    for i in 0..3 {
        t.add(i, i, 2.0);
    }
    dm.assemble_preassembled_matrix("A", &t).map_err(e2s)?;
    dm.finalize_matrix("A").map_err(e2s)?;
    let rhs = vec![2.0, 4.0, 6.0];
    let mut x = vec![0.0; 3];
    petsc_backend(&mut dm)?
        .set_solver_option("A", "ksp_rtol", "1e-14")
        .map_err(e2s)?;
    dm.solve_linear_system("A", &rhs, &mut x).map_err(e2s)?;
    ensure!(
        x.iter()
            .zip([1.0, 2.0, 3.0])
            .all(|(a, b)| (a - b).abs() < 1e-12),
        "first solve {x:?}"
    );
    let mut t = TermsToAssemble::new("x", "x");
    t.add(0, 2, 1.0); // new entry: profile change
    dm.assemble_preassembled_matrix("A", &t).map_err(e2s)?;
    dm.finalize_matrix("A").map_err(e2s)?;
    dm.solve_linear_system("A", &rhs, &mut x).map_err(e2s)?;
    // [[2,0,1],[0,2,0],[0,0,2]] x = [2,4,6] → x = (−0.5, 2, 3)
    ensure!(
        x.iter()
            .zip([-0.5, 2.0, 3.0])
            .all(|(a, b)| (a - b).abs() < 1e-12),
        "second solve {x:?}"
    );
    let types = petsc_backend(&mut dm)?.ksp_types("A").map_err(e2s)?;
    println!("INFO petsc_profile_change: ksp/pc types {types:?}");
    Ok(())
}

// ------------------------------------------------ options and failures

/// Upstream `ParsablePETSc` rules: akantu names refused with the PETSc
/// option to use, read-only parameters, options from a parser section; the
/// KSP configured by `set_solver_option` (`pc_type jacobi`).
fn petsc_options(c: &Communicator) -> Outcome {
    let (mut model, mut ms) = build(c, Setup::SnesLinear, true, KspConf::Direct)?;
    let nls = ms.non_linear_solver_mut(None).map_err(e2s)?;
    let refused = nls.set_param("max_iterations", ParamValue::Int(3));
    ensure!(
        refused.as_ref().is_err_and(|e| e
            .to_string()
            .contains("use the PETSc option \"snes_max_it\"")),
        "max_iterations not refused: {refused:?}"
    );
    let ro = nls.set_param("n_iter", ParamValue::Int(3));
    ensure!(ro.is_err(), "n_iter writable");
    ms.solve_step(&mut model, None).map_err(e2s)?;
    let nls = ms.non_linear_solver(None).map_err(e2s)?;
    let reason = nls.get_param("convergence_reason").map_err(e2s)?;
    let n_iter = nls.get_param("n_iter").map_err(e2s)?;
    let fnorm = nls.get_param("function_norm").map_err(e2s)?;
    if c.rank() == 0 {
        println!(
            "INFO petsc_options: SNES reason {reason:?}, n_iter {n_iter:?}, function_norm {fnorm:?}"
        );
    }
    ensure!(
        matches!(&reason, ParamValue::String(s) if s.starts_with("CONVERGED")),
        "reason {reason:?}"
    );
    // A parser section: every parameter becomes an option, refused names
    // stay refused.
    let text = "non_linear_solver petsc_snes [\n  snes_type = newtonls\n  ksp_type = preonly\n]\n";
    let parser = akantu_parser::Parser::parse_str(text, "<petsc>").map_err(e2s)?;
    let section = parser
        .root
        .sub_sections_of(akantu_parser::ParserType::NonLinearSolver)
        .next()
        .ok_or("no section")?
        .clone();
    let nls = ms.non_linear_solver_mut(None).map_err(e2s)?;
    match nls.parse_options_section(&section) {
        Some(Ok(())) => {}
        other => return Err(format!("parse_options_section: {other:?}")),
    }
    let native = nls.as_native_mut().ok_or("not native")?;
    ensure!(
        native
            .options()
            .iter()
            .any(|(k, v)| k == "snes_type" && v == "newtonls"),
        "options {:?}",
        native.options()
    );
    let bad = "non_linear_solver petsc_snes [\n  threshold = 1e-3\n]\n";
    let parser = akantu_parser::Parser::parse_str(bad, "<petsc>").map_err(e2s)?;
    let section = parser
        .root
        .sub_sections_of(akantu_parser::ParserType::NonLinearSolver)
        .next()
        .ok_or("no section")?
        .clone();
    let nls = ms.non_linear_solver_mut(None).map_err(e2s)?;
    ensure!(
        matches!(nls.parse_options_section(&section), Some(Err(_))),
        "threshold accepted in a section"
    );
    Ok(())
}

/// A callback failure on one rank inside SNES (rank `p − 1`'s residual
/// assembly at the second evaluation) is reported on every rank, PETSc's
/// control flow stays in lock-step (no hang), and the next solve works.
fn petsc_snes_rank_specific_failure(c: &Communicator) -> Outcome {
    let (mut model, mut ms) = build(c, Setup::SnesCubic, true, KspConf::Direct)?;
    if c.rank() as usize == c.size() - 1 {
        model.fail_at = Some("assemble_residual".into());
    }
    let t0 = Instant::now();
    let r = ms.solve_step(&mut model, None);
    ensure!(r.is_err(), "the failing solve succeeded");
    ensure!(
        t0.elapsed() < Duration::from_secs(60),
        "failure took {:?}",
        t0.elapsed()
    );
    if c.rank() == 0 {
        println!(
            "INFO petsc_snes_rank_specific_failure: rank 0 error: {}",
            r.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }
    model.fail_at = None;
    ms.solve_step(&mut model, None).map_err(e2s)?;
    ensure!(
        ms.non_linear_solver(None).map_err(e2s)?.converged(),
        "the solve after the failure did not converge"
    );
    Ok(())
}

/// A new backend after every PETSc object was dropped joins the PETSc
/// session that stays initialised while the world communicator lives
/// (upstream finalises once, at exit); the deferred destructions are
/// drained first.
fn petsc_session_restart(c: &Communicator) -> Outcome {
    {
        let (mut model, mut ms) = build(c, Setup::StaticNewton, true, KspConf::Direct)?;
        ms.solve_step(&mut model, None).map_err(e2s)?;
    }
    let _ = akantu_comm::mpi_lifecycle().poll();
    let (mut model, mut ms) = build(c, Setup::StaticNewton, true, KspConf::Direct)?;
    ms.solve_step(&mut model, None).map_err(e2s)?;
    drop((model, ms));
    let _ = akantu_comm::mpi_lifecycle().poll();
    // A TAO solve after every PETSc user of the live world
    // was dropped (no re-initialisation: TAO's types stay registered).
    let (mut model, mut ms) = build(c, Setup::TaoLinear, true, KspConf::Direct)?;
    ms.solve_step(&mut model, None).map_err(e2s)?;
    Ok(())
}

/// The default backend has no PETSc solvers; the PETSc backend refuses a
/// non-PETSc sparse solver for its native solvers (upstream message).
/// Upstream `DOFManagerPETSc` allocates its residual,
/// solution, data cache and lumped matrices as `SolverVectorPETSc`, whose
/// `isDistributed()` is always `true` (`solver_vector_petsc.hh:119`,
/// `dof_manager_petsc.cc:287-289, 446-448`), whatever the mesh. The PETSc
/// manager's vectors are of that kind before and after the nodal
/// registration, on the serial bar of one process as on the distributed
/// bar; the default manager on the same bar follows the mesh. Every check
/// is evaluated before the verdict (the registrations are collective, so
/// no rank returns between them, D-106 (2)).
fn lr20_petsc_vectors_are_distributed(c: &Communicator) -> Outcome {
    let (mesh, _) = partitioned_bar(c, 17);
    let mut dm = petsc_dm("dm", c)?;
    let before = (dm.vectors_distributed(), dm.residual().is_distributed());
    let early = dm.new_lumped_matrix("early").map_err(e2s)?.is_distributed();
    let reg = dm.register_nodal_dofs(&mesh, "disp", 1).map_err(e2s);
    let after = (
        dm.vectors_distributed(),
        dm.residual().is_distributed(),
        dm.residual().to_solver_vector().is_distributed(),
        dm.lumped_matrix("early").map_err(e2s)?.is_distributed(),
        dm.new_lumped_matrix("late").map_err(e2s)?.is_distributed(),
    );
    let mut aij = DofManager::new("aij", c.clone());
    let reg_aij = aij.register_nodal_dofs(&mesh, "disp", 1).map_err(e2s);
    reg?;
    reg_aij?;
    ensure!(
        before == (true, true) && early,
        "PETSc manager before the registration: {before:?}, early lumped {early}"
    );
    ensure!(
        after == (true, true, true, true, true),
        "PETSc manager after the registration: {after:?}"
    );
    ensure!(
        aij.vectors_distributed() == mesh.is_distributed()
            && aij.residual().is_distributed() == mesh.is_distributed(),
        "default manager: {} / {} on a mesh distributed = {}",
        aij.vectors_distributed(),
        aij.residual().is_distributed(),
        mesh.is_distributed()
    );
    Ok(())
}

fn petsc_factory_rules(c: &Communicator) -> Outcome {
    let mut dm = DofManager::new("dm", c.clone());
    let r = akantu_solver::new_non_linear_solver(
        &mut dm,
        "s",
        NonLinearSolverType::PetscSnes,
        SparseSolverType::Auto,
    );
    ensure!(r.is_err(), "the AIJ manager created a SNES");
    let mut dm = petsc_dm("dm", c)?;
    let r = akantu_solver::new_non_linear_solver(
        &mut dm,
        "s",
        NonLinearSolverType::PetscSnes,
        SparseSolverType::Eigen,
    );
    ensure!(
        r.as_ref().is_err_and(|e| e
            .to_string()
            .contains("petsc non linear solver works only with petsc sparse solver")),
        "SNES with eigen: {r:?}"
    );
    let r = akantu_solver::new_non_linear_solver(
        &mut dm,
        "s",
        NonLinearSolverType::NewtonRaphson,
        SparseSolverType::Eigen,
    );
    ensure!(r.is_err(), "NR with eigen on the PETSc manager accepted");
    Ok(())
}

// ------------------------------------------------- oracle comparison

/// The authoritative serial oracle record (`oracle/probes/petsc`, x86_64
/// strict build) generated under the D-050 pin: OpenBLAS core Haswell, one
/// thread, Eigen cache sizes 16384/524288/524288. Read
/// at run time from the repository (`serial.x86_64-haswell.json`).
fn oracle() -> std::result::Result<serde_json::Value, String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/solver/petsc/oracle/serial.x86_64-haswell.json"
    );
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// The first record (OpenBLAS core of its host, Eigen cache sizes
/// of its host): **unpinned, diagnostic** only (D-050 (3)).
fn oracle_unpinned() -> std::result::Result<serde_json::Value, String> {
    let text = include_str!("../../../fixtures/solver/petsc/oracle/serial.x86_64.json");
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// The doubles of a probe record `{"bits": [...]}` (bit patterns, never
/// parsed decimals).
fn bits(v: &serde_json::Value) -> std::result::Result<Vec<f64>, String> {
    v["bits"]
        .as_array()
        .ok_or("no bits")?
        .iter()
        .map(|b| {
            let s = b.as_str().ok_or("bits entry")?;
            u64::from_str_radix(s, 16)
                .map(f64::from_bits)
                .map_err(|e| e.to_string())
        })
        .collect()
}

/// `(entries differing in bits, max |diff| / max |oracle|)`.
fn compare(got: &[f64], want: &[f64]) -> (usize, f64) {
    let scale = want
        .iter()
        .fold(0.0_f64, |m, x| m.max(x.abs()))
        .max(f64::MIN_POSITIVE);
    let n = got
        .iter()
        .zip(want)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count()
        + got.len().abs_diff(want.len());
    let m = got
        .iter()
        .zip(want)
        .fold(0.0_f64, |m, (a, b)| m.max((a - b).abs() / scale));
    (n, m)
}

/// Every upstream-compatibility switch of the solver stack on (the oracle
/// runs unpatched upstream).
fn oracle_compat(dm: &mut DofManager) {
    dm.set_upstream_compat(common::drivers::upstream_compat());
}

/// The serial PETSc cases against the oracle (one process; the oracle runs
/// the same upstream classes on the same toy problems, every compat switch
/// on): bitwise for the diagonal KSP solve, the static Newton with the
/// default KSP, the bar's Newton (default and direct KSP), SNES (linear;
/// cubic: upstream's `DIVERGED_LINE_SEARCH` reproduced through UD-428) and
/// TAO; the dynamic run under rule PETSC-DYN
/// (`docs/NUMERICAL_VALIDATION.md`).
fn oracle_serial(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        return Ok(());
    }
    // D-121: the records exist for the pinned x86_64 oracle only; on another
    // platform the scenario reports that it did not compare (the same toy
    // problems are solved and checked by the other scenarios).
    if petsc_comparability(c, "oracle_serial").is_some() {
        return Ok(());
    }
    // The bitwise comparison is only meaningful under the record's BLAS
    // pin and PETSc version (D-050): refused (FAIL)
    // otherwise, never silently relaxed.
    blas_pin_check().map_err(|e| format!("oracle comparison refused: {e}"))?;
    let o = oracle()?;
    let legacy = oracle_unpinned()?;
    let host = &o["host"];
    let want_version: Vec<i64> = host["petsc_version"]
        .as_array()
        .ok_or("record without petsc_version")?
        .iter()
        .filter_map(serde_json::Value::as_i64)
        .collect();
    let (ma, mi, sub) = petsc::version();
    ensure!(
        want_version == [i64::from(ma), i64::from(mi), i64::from(sub)],
        "oracle comparison refused: PETSc {ma}.{mi}.{sub}, record {want_version:?}"
    );
    ensure!(
        host["openblas_core"]
            .as_str()
            .is_some_and(|c| c.eq_ignore_ascii_case(BLAS_PIN.1))
            && host["eigen_cache_sizes"] == serde_json::json!([16384, 524288, 524288]),
        "the record was not generated under the pin: {host}"
    );
    let mut report = Vec::new();
    let mut fails = Vec::new();
    // diag: test_sparse_solver_petsc.
    {
        let (mesh, _) = partitioned_bar(c, 11);
        let mut dm = petsc_dm("test_dof_manager", c)?;
        oracle_compat(&mut dm);
        dm.register_nodal_dofs(&mesh, "x", 1).map_err(e2s)?;
        dm.new_matrix("A", MatrixType::Symmetric).map_err(e2s)?;
        let mut terms = TermsToAssemble::new("x", "x");
        for i in 0..mesh.nb_nodes() {
            let gi = dm
                .local_to_global_equation_number(eq(&dm, "x", i)?)
                .ok_or("ghost")?;
            terms.add(i as Idx, i as Idx, 1.0 / (1.0 + gi as f64));
        }
        dm.assemble_preassembled_matrix("A", &terms).map_err(e2s)?;
        dm.finalize_matrix("A").map_err(e2s)?;
        let b = Array::from_vec(vec![1.0; mesh.nb_nodes()], 1).map_err(e2s)?;
        dm.zero_residual();
        dm.assemble_to_residual("x", b.view(), 1.0).map_err(e2s)?;
        dm.solve("A").map_err(e2s)?;
        let y = dm.dofs_solution("x").map_err(e2s)?.as_slice().to_vec();
        let (n, m) = compare(&y, &bits(&o["diag"]["x"])?);
        report.push(format!("diag: {n} differing bits, max rel {m:.3e}"));
        if n != 0 {
            fails.push("diag not bitwise".to_owned());
        }
    }
    // static_default: test_model_solver_petsc.
    {
        let mut dm = petsc_dm("model_solver:dof_manager_petsc", c)?;
        oracle_compat(&mut dm);
        let mut model = MyModel::new(-10.0, gen_mesh(100), false, &mut dm).map_err(e2s)?;
        let mut ms = ModelSolver::new(dm);
        let opts = ModelSolverOptions {
            time_step_solver_type: Some(TimeStepSolverType::Static),
            non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
            ..ModelSolverOptions::default()
        };
        ms.get_new_solver("static", &opts, &mut model)
            .map_err(e2s)?;
        ms.set_integration_scheme(
            "static",
            "disp",
            IntegrationSchemeType::PseudoTime,
            SolutionType::NotDefined,
            &model,
        )
        .map_err(e2s)?;
        ms.non_linear_solver_mut(Some("static"))
            .map_err(e2s)?
            .set_param("max_iterations", ParamValue::Int(2))
            .map_err(e2s)?;
        let _ = ms.solve_step(&mut model, None);
        let u = ms
            .dof_manager
            .dofs("disp")
            .map_err(e2s)?
            .as_slice()
            .to_vec();
        let nls = ms.non_linear_solver(None).map_err(e2s)?;
        let (n, m) = compare(&u, &bits(&o["static_default"]["displacement"])?);
        let it_ok = o["static_default"]["n_iter"].as_u64() == Some(nls.nb_iterations() as u64);
        let (ne, _) = compare(&[nls.error()], &bits(&o["static_default"]["error"])?);
        report.push(format!(
            "static_default: u {n} differing bits (max rel {m:.3e}), iterations equal {it_ok}, error bits differ {ne}"
        ));
        let (nl, ml) = compare(&u, &bits(&legacy["static_default"]["displacement"])?);
        report.push(format!(
            "static_default vs the unpinned record (diagnostic): u {nl} differing bits (max rel {ml:.3e})"
        ));
        if n != 0 || !it_ok || ne != 0 {
            fails.push("static_default not bitwise".to_owned());
        }
    }
    // dynamic_default: test_model_solver_dynamic_petsc.
    {
        let f = case("dynamic_implicit");
        let dm = petsc_dm("model_solver:dof_manager_petsc", c)?;
        let run = run_dynamic_with(&f, SparseSolverType::Petsc, dm);
        if let Some((step, e)) = &run.failure {
            return Err(format!("dynamic step {step}: {e}"));
        }
        let od = &o["dynamic_default"];
        let (ne, me) = compare(&run.energies, &bits(&od["energies"])?);
        let want_it: Vec<i64> = od["n_iter"]
            .as_array()
            .ok_or("n_iter")?
            .iter()
            .filter_map(serde_json::Value::as_i64)
            .collect();
        let it_ok = want_it == run.n_iter;
        let ms = run.solver.as_ref().ok_or("no solver")?;
        let dm = &ms.dof_manager;
        let (nu, mu) = compare(
            dm.dofs("disp").map_err(e2s)?.as_slice(),
            &bits(&od["displacement"])?,
        );
        let (nv, mv) = compare(
            dm.dofs_derivative("disp", 1).map_err(e2s)?.as_slice(),
            &bits(&od["velocity"])?,
        );
        let (na, ma) = compare(
            dm.dofs_derivative("disp", 2).map_err(e2s)?.as_slice(),
            &bits(&od["acceleration"])?,
        );
        report.push(format!(
            "dynamic_default: energies {ne} differing bits (max rel {me:.3e}), iterations equal {it_ok}, \
             final u {nu} ({mu:.3e}), v {nv} ({mv:.3e}), a {na} ({ma:.3e})"
        ));
        let (nl, ml) = compare(
            &run.energies,
            &bits(&legacy["dynamic_default"]["energies"])?,
        );
        report.push(format!(
            "dynamic_default vs the unpinned record (diagnostic): energies {nl} differing bits \
             (max rel {ml:.3e})"
        ));
        // Rule PETSC-DYN (PROPOSED, NUMERICAL_VALIDATION):
        // identical Newton iteration counts every step, energies within
        // 1e-12 and final u, v, a within 1e-11 of the oracle (relative to
        // each array's maximum magnitude).
        if !it_ok || me > 1e-12 || mu > 1e-11 || mv > 1e-11 || ma > 1e-11 {
            fails.push("dynamic_default outside rule PETSC-DYN".to_owned());
        }
    }
    // bar17 cases.
    for (key, setup, conf) in [
        (
            "bar17_nr_cubic_default",
            Setup::StaticCubicNewton,
            KspConf::Default,
        ),
        (
            "bar17_nr_cubic_direct",
            Setup::StaticCubicNewton,
            KspConf::Direct,
        ),
        (
            "bar17_snes_linear_direct",
            Setup::SnesLinear,
            KspConf::Direct,
        ),
        ("bar17_snes_cubic_direct", Setup::SnesCubic, KspConf::Direct),
        ("bar17_tao_linear_direct", Setup::TaoLinear, KspConf::Direct),
    ] {
        let rec = &o[key];
        let (mut model, mut ms) = build(c, setup, true, conf)?;
        oracle_compat(&mut ms.dof_manager);
        let r = ms.solve_step(&mut model, None);
        let u = ms
            .dof_manager
            .dofs("disp")
            .map_err(e2s)?
            .as_slice()
            .to_vec();
        let (n, m) = compare(&u, &bits(&rec["displacement"])?);
        let nls = ms.non_linear_solver(None).map_err(e2s)?;
        let it = nls.nb_iterations() as u64;
        let want_it = rec["n_iter"].as_u64();
        let mut extra = String::new();
        if key.contains("snes") {
            let fnorm = match nls.get_param("function_norm") {
                Ok(ParamValue::Real(v)) => v,
                _ => f64::NAN,
            };
            let (nf, _) = compare(&[fnorm], &bits(&rec["function_norm"])?);
            let reason = match nls.get_param("convergence_reason") {
                Ok(ParamValue::String(s)) => s,
                _ => String::new(),
            };
            extra = format!(
                ", function_norm bits differ {nf}, reason {reason} vs {}",
                rec["convergence_reason"]
            );
            if nf != 0 || rec["convergence_reason"].as_str() != Some(reason.as_str()) {
                fails.push(format!("{key}: SNES diagnostics differ"));
            }
        }
        report.push(format!(
            "{key}: solve {:?}, oracle status {}, u {n} differing bits (max rel {m:.3e}), iterations {it} vs {want_it:?}{extra}",
            r.as_ref().map(|_| ()).map_err(ToString::to_string),
            rec["status"]
        ));
        // The probe records no TAO iteration count (upstream registers no
        // readable parameter for it).
        let it_ok = want_it.is_none_or(|w| w == it) && (want_it.is_some() || key.contains("tao"));
        if !it_ok || n != 0 {
            fails.push(format!("{key}: differs from the oracle"));
        }
    }
    for line in &report {
        println!("INFO oracle_serial: {line}");
    }
    if fails.is_empty() {
        Ok(())
    } else {
        Err(fails.join("; "))
    }
}

// ------------------------------------------------- upstream defects

/// Solve the partitioned bar with unsymmetric elemental blocks `blk`
/// (column-major 2 × 2), node 0 blocked, load 1 on the last node; the
/// gathered solution on rank 0. PETSc backend (direct KSP) with
/// `row_oriented` = UD-422 compat, or the default backend with Eigen.
fn unsymmetric_bar(
    c: &Communicator,
    petsc: bool,
    row_oriented: bool,
    blk: [f64; 4],
) -> std::result::Result<Vec<(i64, Real)>, String> {
    let (mesh, gids) = partitioned_bar(c, N);
    let mut dm = if petsc {
        petsc_dm("dm", c)?
    } else {
        DofManager::new("dm", c.clone())
    };
    let mut compat = dm.upstream_compat();
    compat.petsc_row_oriented_blocks = row_oriented;
    dm.set_upstream_compat(compat);
    dm.register_nodal_dofs(&mesh, "u", 1).map_err(e2s)?;
    dm.register_blocked_dofs("u").map_err(e2s)?;
    for (k, g) in gids.iter().enumerate() {
        if *g == 0 {
            dm.blocked_dofs_mut("u").map_err(e2s)?.as_mut_slice()[k] = true;
        }
    }
    dm.new_matrix("A", MatrixType::Unsymmetric).map_err(e2s)?;
    let mut k = Array::new(4);
    for _ in 0..mesh.nb_element(SEG, GhostType::NotGhost) {
        k.push_tuple(&blk);
    }
    dm.assemble_elemental_matrices_to_matrix(
        &mesh,
        "A",
        "u",
        k.view(),
        SEG,
        GhostType::NotGhost,
        MatrixType::Unsymmetric,
        None,
    )
    .map_err(e2s)?;
    dm.finalize_matrix("A").map_err(e2s)?;
    dm.update_global_blocked_dofs();
    dm.apply_boundary("A").map_err(e2s)?;
    let n_local = dm.local_system_size();
    let mut rhs = vec![0.0; n_local];
    for (k, g) in gids.iter().enumerate() {
        if *g == N as i64 - 1 {
            rhs[eq(&dm, "u", k)?] = 1.0;
        }
    }
    if petsc {
        for (o, v) in ksp_options(KspConf::Direct, c.size()) {
            petsc_backend(&mut dm)?
                .set_solver_option("A", o, v)
                .map_err(e2s)?;
        }
    } else {
        dm.backend_mut()
            .map_err(e2s)?
            .set_sparse_solver(
                "A",
                akantu_linalg::SparseSolverFactory::default()
                    .create(SparseSolverType::Eigen, 1)
                    .map_err(e2s)?,
            )
            .map_err(e2s)?;
    }
    let mut x = vec![0.0; n_local];
    dm.solve_linear_system("A", &rhs, &mut x).map_err(e2s)?;
    let xs: Vec<f64> = (0..gids.len())
        .map(|k| eq(&dm, "u", k).map(|l| x[l]))
        .collect::<std::result::Result<_, _>>()?;
    Ok(gather_by_id(c, &gids, &xs))
}

/// UD-422: upstream reads every column-major elemental block row-major
/// (`MAT_ROW_ORIENTED`), i.e. assembles the transposed blocks. Default: the
/// PETSc solution equals the default backend's (Eigen) for the same
/// blocks; compat: it equals Eigen's for the transposed blocks.
fn ud422_row_oriented_blocks(c: &Communicator) -> Outcome {
    let blk = [3.0, -2.0, -1.0, 4.0]; // (0,0)=3 (1,0)=−2 (0,1)=−1 (1,1)=4
    let blk_t = [3.0, -1.0, -2.0, 4.0];
    let default = unsymmetric_bar(c, true, false, blk)?;
    let compat = unsymmetric_bar(c, true, true, blk)?;
    if c.rank() == 0 {
        let serial = Communicator::serial();
        let eigen = unsymmetric_bar(&serial, false, false, blk)?;
        let eigen_t = unsymmetric_bar(&serial, false, false, blk_t)?;
        let (r_def, r_compat, r_cross) = (
            max_rel(&default, &eigen),
            max_rel(&compat, &eigen_t),
            max_rel(&compat, &eigen),
        );
        println!(
            "INFO ud422: ranks={} default vs Eigen {r_def:?}, compat vs Eigen(Bᵀ) {r_compat:?},              compat vs Eigen(B) {r_cross:?}",
            c.size()
        );
        ensure!(r_def.is_some_and(|r| r <= 1e-12), "default {r_def:?}");
        ensure!(r_compat.is_some_and(|r| r <= 1e-12), "compat {r_compat:?}");
        ensure!(
            r_cross.is_some_and(|r| r > 1e-3),
            "compat equals the untransposed solve"
        );
    }
    Ok(())
}

/// UD-423: a KSP that stops on its iteration limit (`ksp_max_it 1`,
/// unpreconditioned GMRES) is an `Error::LinearSolver` on every rank by
/// default; with the compat switch the last iterate is returned silently,
/// as upstream.
fn ud423_ksp_divergence(c: &Communicator) -> Outcome {
    let mut outcomes = Vec::new();
    for compat_on in [false, true] {
        let (mesh, _) = partitioned_bar(c, N);
        let mut dm = petsc_dm("dm", c)?;
        let mut compat = dm.upstream_compat();
        compat.petsc_ignore_ksp_divergence = compat_on;
        dm.set_upstream_compat(compat);
        dm.register_nodal_dofs(&mesh, "u", 1).map_err(e2s)?;
        dm.new_matrix("A", MatrixType::Symmetric).map_err(e2s)?;
        let mut k = Array::new(4);
        for _ in 0..mesh.nb_element(SEG, GhostType::NotGhost) {
            k.push_tuple(&[2.0, -1.0, -1.0, 2.0]);
        }
        dm.assemble_elemental_matrices_to_matrix(
            &mesh,
            "A",
            "u",
            k.view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
        .map_err(e2s)?;
        dm.finalize_matrix("A").map_err(e2s)?;
        for (o, v) in [
            ("ksp_type", "gmres"),
            ("pc_type", "none"),
            ("ksp_rtol", "1e-14"),
            ("ksp_max_it", "1"),
        ] {
            petsc_backend(&mut dm)?
                .set_solver_option("A", o, v)
                .map_err(e2s)?;
        }
        let n = dm.local_system_size();
        let rhs: Vec<f64> = (0..n).map(|l| 1.0 + l as f64).collect();
        let mut x = vec![0.0; n];
        let r = dm.solve_linear_system("A", &rhs, &mut x);
        let reason = petsc_backend(&mut dm)?
            .last_ksp_outcome("A")
            .map(|o| o.reason);
        outcomes.push((compat_on, r.is_ok(), reason));
        match (compat_on, r) {
            (false, Err(Error::LinearSolver(m))) => {
                ensure!(m.contains("did not converge"), "message {m}");
            }
            (false, other) => return Err(format!("default: {other:?}")),
            (true, Ok(())) => {}
            (true, Err(e)) => return Err(format!("compat: {e}")),
        }
        ensure!(reason.is_some_and(|r| r < 0), "KSP reason {reason:?}");
    }
    if c.rank() == 0 {
        println!(
            "INFO ud423: ranks={} (compat, ok, reason) {outcomes:?}",
            c.size()
        );
    }
    Ok(())
}

/// UD-427: with the reactions kept in the KSP right-hand side (compat), the
/// default (iterative) KSP's relative tolerance is dominated by them and
/// the cubic bar's Newton–Raphson stalls on several ranks (block Jacobi is
/// inexact; on one process ILU(0) is exact on the tridiagonal system and
/// both converge); default: converges like the direct solve.
fn ud427_reactions_in_ksp_rhs(c: &Communicator) -> Outcome {
    let mut results = Vec::new();
    for compat_on in [false, true] {
        let (mut model, mut ms) = build(c, Setup::StaticCubicNewton, true, KspConf::Default)?;
        let mut compat = ms.dof_manager.upstream_compat();
        compat.petsc_ksp_rhs_with_reactions = compat_on;
        ms.dof_manager.set_upstream_compat(compat);
        let r = ms.solve_step(&mut model, None);
        let n = ms.non_linear_solver(None).map_err(e2s)?.nb_iterations();
        results.push((compat_on, r.is_ok(), n));
        match (compat_on, &r) {
            (false, Ok(())) => {}
            (false, Err(e)) => return Err(format!("default: {e}")),
            (true, Err(Error::NotConverged { .. })) if c.size() > 1 => {}
            (true, Ok(())) if c.size() == 1 => {}
            (true, other) => return Err(format!("compat on {} ranks: {other:?}", c.size())),
        }
    }
    if c.rank() == 0 {
        println!(
            "INFO ud427: ranks={} (compat, converged, iterations) {results:?}",
            c.size()
        );
    }
    Ok(())
}

/// The D-050 BLAS pin under which the PETSc oracle fixtures were generated
/// (platform, `OPENBLAS_CORETYPE`, one thread).
const BLAS_PIN: (&str, &str) = ("x86_64", "Haswell");

/// The environment of the PETSc oracle records ([`BLAS_PIN`]).
const PETSC_RECORD_ENV: &str = "x86_64-haswell";

/// D-121: `None` when this process compares with the PETSc oracle records
/// (always on x86_64, where a missing pin still fails the comparison);
/// otherwise the reason, after printing the scenario's report line
/// (rank 0).
fn petsc_comparability(c: &Communicator, scenario: &str) -> Option<String> {
    let not_comparable = oracle_values_not_comparable(PETSC_RECORD_ENV);
    if c.rank() == 0 {
        println!(
            "{}",
            oracle_report_line(scenario, PETSC_RECORD_ENV, not_comparable.as_deref())
        );
    }
    not_comparable
}

/// `Ok` iff this process runs the pinned OpenBLAS configuration of the
/// oracle fixtures: on x86_64 `OPENBLAS_CORETYPE=Haswell` and
/// `OPENBLAS_NUM_THREADS=1`, with OpenBLAS reporting that core and one
/// thread. Other platforms have no PETSc fixture set.
fn blas_pin_check() -> std::result::Result<(), String> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let blas = petsc::blas_info();
    if std::env::consts::ARCH != BLAS_PIN.0 {
        return Err(format!(
            "no PETSc oracle fixture set for {} (only {} with OPENBLAS_CORETYPE={})",
            std::env::consts::ARCH,
            BLAS_PIN.0,
            BLAS_PIN.1
        ));
    }
    let threads = env("OPENBLAS_NUM_THREADS");
    if threads.as_deref() != Some("1") || blas.threads.is_some_and(|t| t != 1) {
        return Err(format!(
            "OPENBLAS_NUM_THREADS = {threads:?} (OpenBLAS reports {:?} threads); the fixtures \
             need 1 (D-050)",
            blas.threads
        ));
    }
    let core = env("OPENBLAS_CORETYPE");
    if core.as_deref() != Some(BLAS_PIN.1) {
        return Err(format!(
            "OPENBLAS_CORETYPE = {core:?}, the fixtures were generated with {} (D-050 BLAS pin \
             absent or different)",
            BLAS_PIN.1
        ));
    }
    if !blas
        .core
        .as_deref()
        .is_some_and(|c| c.eq_ignore_ascii_case(BLAS_PIN.1))
    {
        return Err(format!(
            "OpenBLAS runs core {:?} although OPENBLAS_CORETYPE = {}",
            blas.core, BLAS_PIN.1
        ));
    }
    Ok(())
}

/// D-050: fails unless the BLAS pin of the oracle fixtures is in
/// effect (environment and the core OpenBLAS actually runs).
fn blas_pin(c: &Communicator) -> Outcome {
    let blas = petsc::blas_info();
    if c.rank() == 0 {
        println!(
            "INFO blas: platform={} OPENBLAS_CORETYPE={:?} OPENBLAS_NUM_THREADS={:?} \
             openblas_core={:?} openblas_threads={:?} pin={BLAS_PIN:?}",
            std::env::consts::ARCH,
            std::env::var("OPENBLAS_CORETYPE").ok(),
            std::env::var("OPENBLAS_NUM_THREADS").ok(),
            blas.core,
            blas.threads,
        );
    }
    // D-121: the pin is the precondition of the oracle comparisons, which do
    // not run on another platform.
    if petsc_comparability(c, "blas_pin").is_some() {
        return Ok(());
    }
    blas_pin_check()
}

fn main() {
    if let Ok(case) = std::env::var("AKANTU_PETSC_CASE") {
        std::process::exit(petsc_mpi_cases::run(&case));
    }
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(1500));
        eprintln!("petsc_mpi: watchdog expired");
        std::process::exit(70);
    });
    let control = world();
    let work = world();
    let rank = control.rank();
    let scenarios: [Scenario; 10] = [
        ("petsc_factory_rules", petsc_factory_rules),
        ("test_sparse_solver_petsc", test_sparse_solver_petsc),
        ("test_model_solver_petsc", test_model_solver_petsc),
        (
            "test_model_solver_dynamic_petsc",
            test_model_solver_dynamic_petsc,
        ),
        ("petsc_matrix_two_families", petsc_matrix_two_families),
        ("petsc_profile_change", petsc_profile_change),
        ("partitioned_bar_newton", partitioned_bar_newton),
        ("partitioned_bar_native", partitioned_bar_native),
        ("petsc_options", petsc_options),
        (
            "petsc_snes_rank_specific_failure",
            petsc_snes_rank_specific_failure,
        ),
    ];
    let only = std::env::var("AKANTU_PETSC_ONLY").ok();
    let mut failed = 0;
    let mut run_list: Vec<Scenario> = scenarios.to_vec();
    run_list.push(("blas_pin", blas_pin));
    run_list.push(("oracle_serial", oracle_serial));
    run_list.push(("ud422_row_oriented_blocks", ud422_row_oriented_blocks));
    run_list.push(("ud423_ksp_divergence", ud423_ksp_divergence));
    run_list.push(("ud427_reactions_in_ksp_rhs", ud427_reactions_in_ksp_rhs));
    run_list.push((
        "test_dof_manager_petsc",
        petsc_pftao::dof_manager::test_dof_manager_petsc,
    ));
    run_list.push((
        "petsc_tao_bounds",
        petsc_pftao::tao_bounds::petsc_tao_bounds,
    ));
    run_list.push((
        "petsc_tao_not_converged",
        petsc_pftao::tao_bounds::petsc_tao_not_converged,
    ));
    run_list.push((
        "petsc_tao_ud1022",
        petsc_pftao::tao_bounds::petsc_tao_ud1022,
    ));
    run_list.push((
        "petsc_tao_ud1022_refused",
        petsc_pftao::tao_bounds::petsc_tao_ud1022_refused,
    ));
    run_list.push(("ua01_add_row_range", petsc_unsafe_fix::ua01_add_row_range));
    run_list.push(("ua03_ksp_layout", petsc_unsafe_fix::ua03_ksp_layout));
    run_list.push(("ao_map_range", petsc_unsafe_fix::ao_map_range));
    run_list.push((
        "lr20_petsc_vectors_are_distributed",
        lr20_petsc_vectors_are_distributed,
    ));
    run_list.push(("petsc_session_restart", petsc_session_restart));
    for (name, f) in run_list {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
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
        println!("petsc_mpi: failed={failed}");
    }
    drop(work);
    drop(control);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
