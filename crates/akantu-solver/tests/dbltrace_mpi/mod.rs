// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test of behaviour ported from Akantu
// (EPFL/LSMS): src/solver/solver_vector_distributed.cc:83-95
// (SolverVectorDistributed::saveVector), src/solver/sparse_matrix_aij.cc:
// 125-150 (saveMatrix), src/model/common/non_linear_solver/
// non_linear_solver_newton_raphson.cc:101-164 (the dblTrace dumps),
// src/model/common/dof_manager/dof_manager.cc:1014-1034 (saveAssociatedNodes),
// src/model/common/dof_manager/dof_manager_default.cc:52-73 (distributed
// vectors on a distributed mesh), against the oracle case
// solver_model/dbltrace_newton_raphson_np2 (oracle/probes/solver_model/
// solver_model_probe.cc, probe v5) and the toy of test/test_model/
// test_common/test_model_solver/test_model_solver_my_model.hh.

//! The Newton–Raphson `dblTrace` dumps on a distributed mesh under real MPI,
//! scenario of `mumps_mpi_solver` run at 1, 2 and 4
//! processes. It runs **first** in its process: upstream's `first` flag and
//! dump counter are process-wide and fresh there, as in the oracle's
//! processes.
//!
//! The bar of 11 nodes is distributed by the port's `distribute` with the
//! contiguous partition (element `e` on rank `e·p / 10`: the oracle's
//! `MeshPartitionMeshData` halves at 2 processes), and solved with the
//! probe's non-linear law, two load steps of −2, the solution criterion and
//! MUMPS, every rank dumping into one scratch directory. Checked on every
//! rank count: the vectors are of the distributed kind; with one process the
//! names have no suffix (those of the serial golden), with several every
//! file carries the writing rank's `_rank-<r>` suffix and every rank wrote
//! `nodes`, and `A`, `b`, `x` for each Newton iteration numbered from 0;
//! the ranks' `b` / `x` rows cover every global equation once. At 2
//! processes against the oracle: the local nodes (global ids, flags), the
//! iteration counts, the displacements bitwise, each rank's file names and
//! the directory's listing, and every file byte for byte (upstream's Matrix
//! Market precision, UD-061). At 2 processes the toy runs with
//! `MyModel::upstream_partial_forces` (the oracle probe's `NonLinearBar`
//! exactly: its residual override drops the ghost-element exchange of
//! upstream's `MyModel`, so its forces are rank-partial at the shared node);
//! at 4 that system does not converge in 10 Newton
//! iterations (three shared nodes), so the 4-process run solves the
//! completed one (upstream `MyModel`'s completion with the probe's law). It
//! has no oracle: the kind, the names and the rows are checked, no value.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::debug::{self, DebugLevel};
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{GhostType, Idx};
use akantu_linalg::matrix_market::MatrixMarketOptions;
use akantu_mesh::distribution::{MeshPartitioner, PrescribedPartition, distribute_with};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};
use akantu_validation::Fixture;
use akantu_validation::fixture::{oracle_report_line, oracle_values_not_comparable};

use crate::common::drivers::upstream_compat;
use crate::common::my_model::{Law, MyModel, SEG};

type Outcome = std::result::Result<(), String>;

/// Nodes of the bar (the oracle case's `nb_nodes`).
const N: usize = 11;
/// The oracle case of each rank at 2 processes.
const ORACLE_NP2: &str = "solver_model/dbltrace_newton_raphson_np2";

fn e2s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A check whose outcome may differ between ranks, agreed before the next
/// collective call (D-106 (2)): every rank fails when one does, the failing
/// ones with their own message.
fn agree(c: &Communicator, local: Outcome) -> Outcome {
    let mut n = [u64::from(local.is_err())];
    c.all_reduce(&mut n, ReduceOp::Sum).map_err(e2s)?;
    match local {
        Err(e) => Err(e),
        Ok(()) if n[0] > 0 => Err(format!("a check failed on {} other rank(s)", n[0])),
        Ok(()) => Ok(()),
    }
}

/// `genMesh(N)` on rank 0, distributed over `c` with the contiguous
/// partition (collective; `distribute` marks the mesh distributed on one
/// process too, as upstream).
fn distributed_bar(c: &Communicator) -> std::result::Result<Mesh, String> {
    let mut mesh = Mesh::with_communicator(1, "bar", c.clone()).map_err(e2s)?;
    if c.rank() == 0 {
        let mut acc = MeshAccessor::new(&mut mesh);
        for n in 0..N {
            acc.push_node(&[n as f64 * (1.0 / (N - 1) as f64)])
                .map_err(e2s)?;
        }
        let mut conn = acc.connectivity_mut(SEG, GhostType::NotGhost);
        for n in 0..N - 1 {
            conn.push_tuple(&[n as Idx, n as Idx + 1]);
        }
    }
    let p = c.size();
    let parts: Vec<Idx> = (0..N - 1).map(|e| (e * p / (N - 1)) as Idx).collect();
    let mut partition = PrescribedPartition::new(parts);
    // the model is built afterwards: the events have no consumer yet
    let _ = distribute_with(
        &mut mesh,
        Some(&mut partition as &mut dyn MeshPartitioner),
        0,
    )
    .map_err(e2s)?;
    Ok(mesh)
}

/// What one rank did: its local nodes, iterations, displacements, the
/// vectors' kind.
struct RankRun {
    gids: Vec<i64>,
    flags: Vec<i64>,
    n_iter: Vec<i64>,
    u_hist: Vec<f64>,
    kinds: (bool, bool),
}

/// The traced solve in `dir` (the working directory of the process during
/// the solves; restored afterwards).
fn traced_solve(c: &Communicator, dir: &Path) -> std::result::Result<RankRun, String> {
    let mesh = distributed_bar(c)?;
    let gids: Vec<i64> = (0..mesh.nb_nodes())
        .map(|n| mesh.node_global_id(n as Idx))
        .collect();
    let flags: Vec<i64> = (0..mesh.nb_nodes())
        .map(|n| i64::from(mesh.node_flag(n as Idx).0))
        .collect();
    let mut dm = DofManager::new("model_solver:dof_manager_default", c.clone());
    dm.set_upstream_compat(upstream_compat());
    let mut model =
        MyModel::with_global_ids(0.0, mesh, false, &mut dm, gids.clone(), N as i64).map_err(e2s)?;
    model.law = Law::Cubic;
    // the probe's `NonLinearBar` exactly at 2 processes (the oracle's run;
    // upstream's `MyModel` completes the shared nodes); at 4 its
    // rank-partial rows at three shared nodes keep Newton from converging
    // in 10 iterations (measured: error 0.16), so the 4-process run, which
    // has no oracle, solves the completed system (no shared node at 1)
    model.upstream_partial_forces = c.size() == 2;
    model.tracing = false;
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Mumps),
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
    {
        let nls = ms.non_linear_solver_mut(None).map_err(e2s)?;
        nls.set_param("max_iterations", ParamValue::Int(10))
            .map_err(e2s)?;
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
            .map_err(e2s)?;
        let nr = nls
            .as_newton_raphson_mut()
            .ok_or("not a Newton-Raphson solver")?;
        nr.set_legacy_hooks(true);
        nr.set_trace_matrix_market_options(MatrixMarketOptions::upstream_precision());
    }
    let kinds = (
        ms.dof_manager.vectors_distributed(),
        ms.dof_manager.residual().is_distributed(),
    );
    let cwd = std::env::current_dir().map_err(e2s)?;
    agree(c, std::env::set_current_dir(dir).map_err(e2s))
        .inspect_err(|_| drop(std::env::set_current_dir(&cwd)))?;
    debug::set_debug_level(DebugLevel::TRACE);
    let mut n_iter = Vec::new();
    let mut u_hist = Vec::new();
    let solved = (|| -> Outcome {
        for s in 1..=2_i32 {
            for (k, g) in model.global_ids.clone().iter().enumerate() {
                if *g == N as i64 - 1 {
                    model.forces[k] = -2.0 * f64::from(s);
                }
            }
            // collective; a failure is agreed by the solver on every rank
            ms.solve_step(&mut model, None).map_err(e2s)?;
            let it = ms.non_linear_solver_mut(None).map_err(e2s)?.nb_iterations();
            n_iter.push(i64::try_from(it).map_err(e2s)?);
            u_hist.extend_from_slice(ms.dof_manager.dofs("disp").map_err(e2s)?.as_slice());
        }
        Ok(())
    })();
    debug::set_debug_level(DebugLevel::WARNING);
    let back = std::env::set_current_dir(&cwd).map_err(e2s);
    solved?;
    back?;
    Ok(RankRun {
        gids,
        flags,
        n_iter,
        u_hist,
        kinds,
    })
}

fn listing(dir: &Path) -> std::result::Result<Vec<String>, String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).map_err(e2s)? {
        v.push(e.map_err(e2s)?.file_name().to_string_lossy().into_owned());
    }
    v.sort();
    Ok(v)
}

/// `<X>_<k>[_rank-<r>].mtx` → `(X, k, r)`.
fn parse(name: &str) -> Option<(String, Option<usize>, Option<usize>)> {
    let stem = name.strip_suffix(".mtx")?;
    let (stem, rank) = match stem.split_once("_rank-") {
        Some((s, r)) => (s, Some(r.parse().ok()?)),
        None => (stem, None),
    };
    if stem == "nodes" {
        return Some(("nodes".into(), None, rank));
    }
    let (x, k) = stem.split_once('_')?;
    Some((x.to_owned(), Some(k.parse().ok()?), rank))
}

/// The global rows of a vector file (after its two header lines).
fn rows(path: &Path) -> std::result::Result<Vec<usize>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines()
        .skip(2)
        .map(|l| {
            l.split_whitespace()
                .next()
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| format!("{}: bad line {l:?}", path.display()))
        })
        .collect()
}

/// The checks every rank makes on the shared directory.
fn check_names(
    c: &Communicator,
    run: &RankRun,
    names: &[String],
    dir: &Path,
) -> std::result::Result<Vec<String>, String> {
    let (r, p) = (c.rank() as usize, c.size());
    if run.kinds != (true, true) {
        return Err(format!(
            "vectors' kind {:?} on a distributed mesh",
            run.kinds
        ));
    }
    let total: i64 = run.n_iter.iter().sum();
    let mut by_rank: BTreeMap<Option<usize>, BTreeMap<String, BTreeSet<usize>>> = BTreeMap::new();
    let mut nodes = BTreeSet::new();
    for name in names {
        let (x, k, q) = parse(name).ok_or_else(|| format!("unexpected file {name}"))?;
        if (p == 1) != q.is_none() || q.is_some_and(|q| q >= p) {
            return Err(format!("{name}: wrong suffix for {p} process(es)"));
        }
        match k {
            None => {
                nodes.insert(q);
            }
            Some(k) => {
                if !matches!(x.as_str(), "A" | "b" | "x") {
                    return Err(format!("unexpected file {name}"));
                }
                by_rank
                    .entry(q)
                    .or_default()
                    .entry(x)
                    .or_default()
                    .insert(k);
            }
        }
    }
    let ranks: BTreeSet<Option<usize>> = if p == 1 {
        [None].into()
    } else {
        (0..p).map(Some).collect()
    };
    if nodes != ranks {
        return Err(format!("nodes files of {nodes:?}, want one per process"));
    }
    let want: BTreeSet<usize> = (0..usize::try_from(total).map_err(e2s)?).collect();
    for q in &ranks {
        for x in ["A", "b", "x"] {
            let got = by_rank.get(q).and_then(|m| m.get(x));
            if got != Some(&want) {
                return Err(format!("{x} dumps of rank {q:?}: {got:?}, want 0..{total}"));
            }
        }
    }
    // the first dump of every rank: its owned rows, every global row once
    for x in ["b", "x"] {
        let mut seen = BTreeSet::new();
        for q in &ranks {
            let suffix = q.map_or(String::new(), |q| format!("_rank-{q}"));
            for g in rows(&dir.join(format!("{x}_0{suffix}.mtx")))? {
                if !seen.insert(g) {
                    return Err(format!("{x}_0: row {g} written by two ranks"));
                }
            }
        }
        if seen != (1..=N).collect::<BTreeSet<_>>() {
            return Err(format!("{x}_0: rows {seen:?}, want 1..={N}"));
        }
    }
    let mine: Vec<String> = names
        .iter()
        .filter(|n| p == 1 || n.ends_with(&format!("_rank-{r}.mtx")))
        .cloned()
        .collect();
    Ok(mine)
}

fn json_names(f: &Fixture, key: &str) -> std::result::Result<Vec<String>, String> {
    f.manifest().parameters[key]
        .as_array()
        .ok_or_else(|| format!("{}: parameter {key}", f.case_id()))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{key}: not a string"))
        })
        .collect()
}

/// Rank `r` at 2 processes against the oracle's rank `r`.
fn check_oracle_np2(
    c: &Communicator,
    run: &RankRun,
    names: &[String],
    mine: &[String],
    dir: &Path,
) -> Outcome {
    let f = Fixture::open_by_id(&format!("{ORACLE_NP2}/rank{}", c.rank())).map_err(e2s)?;
    f.require_golden().map_err(e2s)?;
    f.verify_checksums().map_err(e2s)?;
    let p = &f.manifest().parameters;
    let flag = |k: &str| p[k].as_bool();
    if (
        flag("residual_is_distributed"),
        flag("solution_is_distributed"),
    ) != (Some(run.kinds.1), Some(run.kinds.0))
    {
        return Err(format!("vectors' kind {:?} vs the oracle's", run.kinds));
    }
    let i64s = |name: &str| f.load_i64(name).map(|a| a.data).map_err(e2s);
    if i64s("nodes/global_id")? != run.gids || i64s("nodes/flag")? != run.flags {
        return Err(format!(
            "local nodes differ from the oracle's: gids {:?} flags {:?} vs {:?} {:?}",
            run.gids,
            run.flags,
            i64s("nodes/global_id")?,
            i64s("nodes/flag")?
        ));
    }
    if i64s("newton/nb_iterations")? != run.n_iter {
        return Err(format!(
            "iterations {:?} vs the oracle's {:?}",
            run.n_iter,
            i64s("newton/nb_iterations")?
        ));
    }
    let check_names = || -> Outcome {
        if json_names(&f, "trace_files")? != mine {
            return Err(format!(
                "this rank's files {mine:?} vs the oracle's {:?}",
                json_names(&f, "trace_files")?
            ));
        }
        if json_names(&f, "all_trace_files")? != names {
            return Err(format!(
                "the directory {names:?} vs the oracle's {:?}",
                json_names(&f, "all_trace_files")?
            ));
        }
        Ok(())
    };
    // D-121: the displacements and the trace files' contents depend on the
    // OpenBLAS kernels inside MUMPS, the local nodes, the iteration counts
    // and the file names do not (an aarch64 oracle run gives the same ones).
    // Outside the record's environment only the latter are compared.
    let recorded = f.environment();
    let not_comparable = oracle_values_not_comparable(&recorded);
    if c.rank() == 0 {
        println!(
            "{}",
            oracle_report_line(
                "lr20_dbltrace_rank_files",
                &recorded,
                not_comparable.as_deref()
            )
        );
    }
    if not_comparable.is_some() {
        return check_names();
    }
    let u = f.load_f64("displacement").map_err(e2s)?.data;
    let differ = u
        .iter()
        .zip(&run.u_hist)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    if u.len() != run.u_hist.len() || differ > 0 {
        return Err(format!(
            "displacements: {differ} of {} differ from the oracle's ({} values)",
            u.len(),
            run.u_hist.len()
        ));
    }
    check_names()?;
    for name in mine {
        let ours = std::fs::read(dir.join(name)).map_err(e2s)?;
        let theirs =
            std::fs::read(f.input_path(&format!("trace:{name}")).map_err(e2s)?).map_err(e2s)?;
        if ours != theirs {
            return Err(format!(
                "{name} differs from the oracle's:\n--- port\n{}\n--- oracle\n{}",
                String::from_utf8_lossy(&ours),
                String::from_utf8_lossy(&theirs)
            ));
        }
    }
    Ok(())
}

/// At one process the names are the serial golden's (that golden was made
/// with Eigen; names only: the iteration counts are the same).
fn check_serial_names(names: &[String]) -> Outcome {
    let f = Fixture::open_by_id("solver_model/dbltrace_newton_raphson").map_err(e2s)?;
    let want = json_names(&f, "trace_files")?;
    if want != names {
        return Err(format!("names {names:?} vs the serial golden's {want:?}"));
    }
    Ok(())
}

/// The scenario (module docs).
pub fn lr20_dbltrace_rank_files(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank(), c.size());
    // one scratch directory for every rank of the job (rank 0's process id)
    let mut id = [u64::from(std::process::id())];
    c.broadcast(&mut id, 0).map_err(e2s)?;
    let dir: PathBuf = std::env::temp_dir().join(format!("akantu-lr20-dbltrace-np{p}-{}", id[0]));
    let prep = if r == 0 {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(e2s)
    } else {
        Ok(())
    };
    agree(c, prep)?;
    let run = traced_solve(c, &dir);
    // every rank's files are written (the solve's errors are agreed)
    let run = agree(c, run.as_ref().map(|_| ()).map_err(Clone::clone)).and(run)?;
    c.barrier().map_err(e2s)?;
    let checked = (|| -> Outcome {
        let names = listing(&dir)?;
        let mine = check_names(c, &run, &names, &dir)?;
        match p {
            1 => check_serial_names(&names),
            2 => check_oracle_np2(c, &run, &names, &mine, &dir),
            _ => Ok(()),
        }
    })();
    let checked = agree(c, checked);
    c.barrier().map_err(e2s)?;
    if r == 0 {
        let _ = std::fs::remove_dir_all(&dir);
    }
    if r == 0 && checked.is_ok() {
        println!(
            "INFO lr20_dbltrace_rank_files: {p} process(es), iterations {:?}{}",
            run.n_iter,
            if p == 2 {
                ", files and values equal to the oracle's"
            } else {
                ""
            }
        );
    }
    checked
}
