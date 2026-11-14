// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests of behaviour ported from Akantu
// (EPFL/LSMS): src/model/common/dof_manager/dof_manager_default.cc:52-73,
// 157-163 (SolverVectorDistributed iff the mesh is distributed),
// src/solver/solver_vector_distributed.cc:73-95 (collective isFinite,
// `_rank-<r>.mtx` file names), src/model/common/dof_manager/dof_manager.cc:
// 1014-1034 (saveAssociatedNodes), src/model/common/non_linear_solver/
// non_linear_solver_newton_raphson.cc:101-164 (the dblTrace dumps),
// src/mesh/mesh.cc:528-558 (distribute marks the mesh distributed on one
// process too), and the toy model of test/test_model/test_common/
// test_model_solver/test_model_solver_my_model.hh.

//! The kind of the DOF manager's vectors on a distributed mesh, on an
//! in-process world of 1, 2 and 4 ranks.
//!
//! Upstream's default DOF manager allocates its residual, solution, data
//! cache and lumped matrices as `SolverVectorDistributed` iff the mesh is
//! distributed (`distribute()` marks it so on one process too); the port
//! binds the manager to the mesh at its first nodal registration (parity
//! row DOF-013). The kind decides what `isDistributed()` answers (Python
//! `SolverVector.isDistributed()` reads it), whether the finiteness check
//! is collective, and whether `saveVector` writes `<name>_rank-<r>.mtx`.
//!
//! The traced Newton solve runs in a **child process** (this binary re-run
//! on that test alone, in a scratch working directory): the debug level,
//! the working directory, upstream's `first` flag and dump counter are
//! process-wide. In the in-process world the ranks are threads of that one
//! process, so they share the counter (each rank's numbers are increasing
//! but interleave with the other ranks') and `first` (one `nodes` file for
//! the process); under MPI every rank is a process of its own and the names
//! are upstream's exactly (`mumps_mpi_solver`, scenario
//! `lr20_dbltrace_rank_files`, against the oracle at 2 ranks).

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use akantu_comm::{Communicator, ReduceOp, run_local};
use akantu_core::debug::{self, DebugLevel};
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{GhostType, Idx};
use akantu_mesh::distribution::{MeshPartitioner, PrescribedPartition, distribute_with};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};

use common::my_model::{Law, MyModel, SEG};

const TIMEOUT: Duration = Duration::from_secs(120);
const RANKS: [usize; 3] = [1, 2, 4];
/// Nodes of the bar (10 segments: 10, 5, 2–3 per rank at 1, 2, 4 ranks).
const N: usize = 11;

const CHILD: &str = "AKANTU_LR20_DBLTRACE_CHILD";
const TRACE_TEST: &str = "lr20_traced_solve_writes_one_file_per_rank";

/// The bar `genMesh(N)` read on rank 0 and distributed over `comm` with the
/// contiguous partition (element `e` on rank `e·p / (N − 1)`); `distribute`
/// marks the mesh distributed on one rank too, as upstream.
fn distributed_bar(comm: &Communicator) -> Mesh {
    let mut mesh = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
    if comm.rank() == 0 {
        let mut acc = MeshAccessor::new(&mut mesh);
        for n in 0..N {
            acc.push_node(&[n as f64 * (1.0 / (N - 1) as f64)]).unwrap();
        }
        let mut c = acc.connectivity_mut(SEG, GhostType::NotGhost);
        for n in 0..N - 1 {
            c.push_tuple(&[n as Idx, n as Idx + 1]);
        }
    }
    let p = comm.size();
    let parts: Vec<Idx> = (0..N - 1).map(|e| (e * p / (N - 1)) as Idx).collect();
    let mut partition = PrescribedPartition::new(parts);
    // no handler exists yet: the distribution events have no consumer
    let _ = distribute_with(
        &mut mesh,
        Some(&mut partition as &mut dyn MeshPartitioner),
        0,
    )
    .unwrap();
    assert!(mesh.is_distributed());
    mesh
}

/// The global node ids of the local nodes.
fn global_ids(mesh: &Mesh) -> Vec<i64> {
    (0..mesh.nb_nodes())
        .map(|n| mesh.node_global_id(n as Idx))
        .collect()
}

/// `true` on every rank iff `b` holds on every rank (an agreed check: the
/// expectations are the same on every rank, D-106 (2)).
fn agreed(comm: &Communicator, b: bool) -> bool {
    let mut v = [u8::from(b)];
    comm.all_reduce(&mut v, ReduceOp::Min).unwrap();
    v[0] == 1
}

/// The vectors' kind before and after the first nodal registration, on a
/// distributed bar at 1, 2 and 4 ranks: lumped matrices created before the
/// registration take the kind when the manager binds to the mesh, those
/// created later are born with it, and the residual's copy keeps it.
/// Fails without the fix (`residual().is_distributed()` was always
/// `false`).
#[test]
fn lr20_vectors_take_the_distribution_of_the_mesh() {
    for p in RANKS {
        let out = run_local(p, TIMEOUT, |c| {
            let mesh = distributed_bar(&c);
            let mut dm = DofManager::new("dm", c.clone());
            let before = (
                dm.vectors_distributed(),
                dm.residual().is_distributed(),
                dm.new_lumped_matrix("early").unwrap().is_distributed(),
            );
            dm.register_nodal_dofs(&mesh, "disp", 1).unwrap();
            let after = (
                dm.vectors_distributed(),
                dm.residual().is_distributed(),
                dm.residual().to_solver_vector().is_distributed(),
                dm.lumped_matrix("early").unwrap().is_distributed(),
                dm.new_lumped_matrix("late").unwrap().is_distributed(),
            );
            // a second family (and the generic one) does not change it
            dm.register_nodal_dofs(&mesh, "temp", 1).unwrap();
            dm.register_generic_dofs("lambda", 2, 1).unwrap();
            let kept = dm.vectors_distributed() && dm.residual().is_distributed();
            let ok = before == (false, false, false) && after == (true, true, true, true, true);
            (agreed(&c, ok && kept), before, after, kept)
        });
        for (r, (ok, before, after, kept)) in out.into_iter().enumerate() {
            assert!(
                ok,
                "{p} ranks, rank {r}: before {before:?}, after {after:?}, kept {kept}"
            );
        }
    }
}

/// On a mesh that is not distributed (serial communicator, no
/// `distribute`) the vectors stay of the default kind, as upstream's
/// `SolverVectorDefault`; so does a manager with generic DOFs only (the
/// port knows no mesh then: parity row DOF-013).
#[test]
fn lr20_undistributed_mesh_and_generic_dofs_keep_the_default_kind() {
    let mut mesh = Mesh::new(1, "bar").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for n in 0..N {
            acc.push_node(&[n as f64]).unwrap();
        }
        let mut c = acc.connectivity_mut(SEG, GhostType::NotGhost);
        for n in 0..N - 1 {
            c.push_tuple(&[n as Idx, n as Idx + 1]);
        }
    }
    assert!(!mesh.is_distributed());
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "disp", 1).unwrap();
    dm.new_lumped_matrix("M").unwrap();
    assert!(!dm.vectors_distributed());
    assert!(!dm.residual().is_distributed());
    assert!(!dm.lumped_matrix("M").unwrap().is_distributed());
    let mut generic = DofManager::new("dm", Communicator::serial());
    generic.register_generic_dofs("lambda", 3, 1).unwrap();
    assert!(!generic.vectors_distributed());
    assert!(!generic.residual().is_distributed());
}

/// The header (`N 1 count`) and the global rows of a vector file.
fn vector_rows(path: &Path) -> (usize, usize, Vec<usize>) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("%%MatrixMarket matrix coordinate real general"),
        "{}",
        path.display()
    );
    let head: Vec<usize> = lines
        .next()
        .unwrap()
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect();
    assert_eq!(head.len(), 3, "{}", path.display());
    assert_eq!(head[1], 1, "{}", path.display());
    let rows: Vec<usize> = lines
        .map(|l| l.split_whitespace().next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(rows.len(), head[2], "{}", path.display());
    (head[0], head[2], rows)
}

/// The rows of the per-rank files of one vector cover every global
/// equation exactly once (each rank writes its owned equations).
fn assert_rows_partition(files: &[(usize, PathBuf)], what: &str) {
    let mut seen = BTreeSet::new();
    for (r, f) in files {
        let (size, _, rows) = vector_rows(f);
        assert_eq!(size, N, "{what}, rank {r}: global size");
        for g in rows {
            assert!(seen.insert(g), "{what}: row {g} written twice");
        }
    }
    assert_eq!(
        seen,
        (1..=N).collect::<BTreeSet<_>>(),
        "{what}: every global row once"
    );
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("akantu-lr20-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Upstream `saveAssociatedNodes` on a distributed bar: `nodes.mtx` on one
/// process, `nodes_rank-<r>.mtx` per rank with several (the lumped vector
/// `"nodes"` is distributed), each rank's file holding its owned rows.
/// Fails without the fix (every rank wrote `nodes.mtx`).
#[test]
fn lr20_associated_nodes_are_saved_per_rank() {
    for p in RANKS {
        let dir = scratch(&format!("nodes-{p}"));
        let d = dir.clone();
        let out = run_local(p, TIMEOUT, move |c| {
            let mesh = distributed_bar(&c);
            let mut dm = DofManager::new("dm", c.clone());
            dm.register_nodal_dofs(&mesh, "disp", 1).unwrap();
            dm.save_associated_nodes(&mesh, &d.join("nodes")).unwrap();
            let owned = (0..mesh.nb_nodes())
                .filter(|&n| mesh.node_flag(n as Idx).is_local_or_master())
                .count();
            (c.rank(), owned, global_ids(&mesh))
        });
        let want: Vec<String> = if p == 1 {
            vec!["nodes.mtx".into()]
        } else {
            (0..p).map(|r| format!("nodes_rank-{r}.mtx")).collect()
        };
        let mut got: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        got.sort();
        assert_eq!(got, want, "{p} ranks: file names");
        let files: Vec<(usize, PathBuf)> = out
            .iter()
            .map(|(r, owned, _)| {
                let f = dir.join(&want[if p == 1 { 0 } else { *r as usize }]);
                assert_eq!(vector_rows(&f).1, *owned, "{p} ranks, rank {r}: rows");
                (*r as usize, f)
            })
            .collect();
        assert_rows_partition(&files, &format!("nodes at {p} ranks"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// The traced static Newton solve of the probe's non-linear bar (cubic law,
/// 2 load steps of −2, solution criterion, Eigen) on the distributed bar
/// at `p` ranks, in the current directory (child).
fn traced_child(p: usize) {
    let out = run_local(p, TIMEOUT, |c| {
        let mesh = distributed_bar(&c);
        let gids = global_ids(&mesh);
        let mut dm = DofManager::new("model_solver:dof_manager_default", c.clone());
        let mut model = MyModel::with_global_ids(0.0, mesh, false, &mut dm, gids, N as i64)
            .map_err(|e| e.to_string())?;
        model.law = Law::Cubic;
        let mut ms = ModelSolver::new(dm);
        let opts = ModelSolverOptions {
            time_step_solver_type: Some(TimeStepSolverType::Static),
            non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        };
        ms.get_new_solver("static", &opts, &mut model)
            .map_err(|e| e.to_string())?;
        ms.set_integration_scheme(
            "static",
            "disp",
            IntegrationSchemeType::PseudoTime,
            SolutionType::NotDefined,
            &model,
        )
        .map_err(|e| e.to_string())?;
        {
            let nls = ms.non_linear_solver_mut(None).map_err(|e| e.to_string())?;
            nls.set_param("max_iterations", ParamValue::Int(10))
                .map_err(|e| e.to_string())?;
            nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
                .map_err(|e| e.to_string())?;
        }
        let mut iterations = 0;
        for s in 1..=2 {
            for (k, g) in model.global_ids.clone().iter().enumerate() {
                model.forces[k] = if *g == N as i64 - 1 {
                    -2.0 * f64::from(s)
                } else {
                    0.0
                };
            }
            ms.solve_step(&mut model, None).map_err(|e| e.to_string())?;
            iterations += ms
                .non_linear_solver_mut(None)
                .map_err(|e| e.to_string())?
                .nb_iterations();
        }
        let kind = ms.dof_manager.vectors_distributed();
        Ok::<_, String>((iterations, kind))
    });
    let mut lines = String::new();
    for (r, o) in out.into_iter().enumerate() {
        let (iterations, kind) = o.unwrap_or_else(|e| panic!("rank {r}: {e}"));
        lines.push_str(&format!(
            "LR20 rank={r} iterations={iterations} kind={kind}\n"
        ));
    }
    print!("{lines}");
}

/// Run this test in a child process (environment `CHILD` = `p`) in a fresh
/// scratch directory; returns the directory and the child's output.
fn run_child(p: usize) -> (PathBuf, String) {
    let dir = scratch(&format!("trace-{p}"));
    let out = Command::new(std::env::current_exe().unwrap())
        .args([TRACE_TEST, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, p.to_string())
        .current_dir(&dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "child ({p} ranks) failed: {}\n{stdout}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    (dir, stdout)
}

/// `<X>_<k>_rank-<r>.mtx` → `(X, k, r)`; `<X>_<k>.mtx` → `(X, k, None)`.
fn parse_trace_name(name: &str) -> Option<(String, usize, Option<usize>)> {
    let stem = name.strip_suffix(".mtx")?;
    let (stem, rank) = match stem.split_once("_rank-") {
        Some((s, r)) => (s, Some(r.parse().ok()?)),
        None => (stem, None),
    };
    let (x, k) = stem.split_once('_')?;
    Some((x.to_owned(), k.parse().ok()?, rank))
}

/// The traced Newton solve on a distributed bar at 1, 2 and 4 ranks: with
/// one rank the names have no suffix (as the serial golden
/// `solver_model/dbltrace_newton_raphson`); with several, every `A_<k>`,
/// `b_<k>`, `x_<k>` and `nodes` file carries the writing rank's suffix,
/// each rank wrote one `A`, `b` and `x` per Newton iteration under the same
/// numbers, and the `b` / `x` files of one rank hold its owned rows (the
/// ranks' rows cover every global equation once). Before the fix the `b_<k>`
/// and `x_<k>` files had no suffix: every rank wrote the same file.
#[test]
fn lr20_traced_solve_writes_one_file_per_rank() {
    if let Some(p) = std::env::var_os(CHILD) {
        let p: usize = p.to_str().unwrap().parse().unwrap();
        debug::set_debug_level(DebugLevel::TRACE);
        traced_child(p);
        debug::set_debug_level(DebugLevel::WARNING);
        return;
    }
    for p in RANKS {
        let (dir, stdout) = run_child(p);
        let mut iterations = BTreeMap::new();
        for l in stdout.lines().filter(|l| l.starts_with("LR20 ")) {
            let f: BTreeMap<&str, &str> = l[5..]
                .split_whitespace()
                .filter_map(|t| t.split_once('='))
                .collect();
            assert_eq!(f["kind"], "true", "{p} ranks: {l}");
            iterations.insert(
                f["rank"].parse::<usize>().unwrap(),
                f["iterations"].parse::<usize>().unwrap(),
            );
        }
        assert_eq!(iterations.len(), p, "{p} ranks: {stdout}");
        assert!(
            stdout.matches("blip\n").count() == 1,
            "{p} ranks: one `blip` per process: {stdout}"
        );
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        // per rank and kind: the dump numbers
        let mut numbers: BTreeMap<(Option<usize>, String), BTreeSet<usize>> = BTreeMap::new();
        let mut nodes = Vec::new();
        for name in &names {
            if name == "nodes.mtx" || name.starts_with("nodes_rank-") {
                nodes.push(name.clone());
                continue;
            }
            let (x, k, r) = parse_trace_name(name).unwrap_or_else(|| panic!("{p} ranks: {name}"));
            assert!(
                matches!(x.as_str(), "A" | "b" | "x"),
                "{p} ranks: unexpected file {name}"
            );
            if p == 1 {
                assert_eq!(r, None, "1 rank: no suffix ({name})");
            } else {
                assert!(
                    r.is_some_and(|r| r < p),
                    "{p} ranks: {name} has no rank suffix of this world"
                );
            }
            assert!(numbers.entry((r, x)).or_default().insert(k), "{name} twice");
        }
        // one `nodes` file for the process (`first` is process-wide)
        assert_eq!(nodes.len(), 1, "{p} ranks: {nodes:?}");
        if p == 1 {
            assert_eq!(nodes[0], "nodes.mtx");
        } else {
            assert!(nodes[0].starts_with("nodes_rank-"), "{nodes:?}");
        }
        let ranks: Vec<Option<usize>> = if p == 1 {
            vec![None]
        } else {
            (0..p).map(Some).collect()
        };
        let mut b_files = BTreeMap::new();
        for r in &ranks {
            let a = &numbers[&(*r, "A".to_owned())];
            assert_eq!(a, &numbers[&(*r, "b".to_owned())], "{p} ranks, rank {r:?}");
            assert_eq!(a, &numbers[&(*r, "x".to_owned())], "{p} ranks, rank {r:?}");
            assert_eq!(
                a.len(),
                iterations[&r.unwrap_or(0)],
                "{p} ranks, rank {r:?}: one dump per Newton iteration"
            );
            let suffix = r.map_or(String::new(), |r| format!("_rank-{r}"));
            b_files.insert(*r, (*a.iter().next().unwrap(), suffix));
        }
        assert_eq!(numbers.len(), 3 * ranks.len(), "{p} ranks: {names:?}");
        // the first dump of each rank: its owned rows, every row once
        for x in ["b", "x"] {
            let files: Vec<(usize, PathBuf)> = b_files
                .iter()
                .map(|(r, (k, suffix))| (r.unwrap_or(0), dir.join(format!("{x}_{k}{suffix}.mtx"))))
                .collect();
            assert_rows_partition(&files, &format!("{x} at {p} ranks"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
        println!("{p} ranks: {} trace files", names.len());
    }
}
