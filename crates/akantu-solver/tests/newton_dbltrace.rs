// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test of the Newton–Raphson `dblTrace` dumps ported from Akantu
// (EPFL/LSMS) src/model/common/non_linear_solver/
// non_linear_solver_newton_raphson.cc:101-108, 147-163 and
// src/model/common/dof_manager/dof_manager.cc:1014-1034 (saveAssociatedNodes),
// against the oracle case solver_model/dbltrace_newton_raphson
// (oracle/probes/solver_model/solver_model_probe.cc, probe v4).

//! DOF-017 (decision D-081): at debug level `TRACE` the Newton loop writes
//! `blip` on standard output and `nodes.mtx` once per process, and
//! `A_<k>.mtx`, `b_<k>.mtx`, `x_<k>.mtx` at every iteration with one
//! process-wide counter. Upstream's statics are per process, so the traced
//! solve runs in a **child process** (this test binary re-run on this test
//! alone, in a scratch working directory): the counter starts at 0 there and
//! the standard output can be read. Every file must equal the oracle's byte
//! for byte with upstream's Matrix Market precision
//! (`MatrixMarketOptions::upstream_precision`, UD-061), and the file set
//! must be the oracle's.
//!
//! The second test runs the first load step **untraced**
//! and traces only the second: upstream advances `dump_counter` at every
//! iteration whether the trace is active or not and consumes `first` only
//! when it is (`non_linear_solver_newton_raphson.cc:101-108, 147-163`), so
//! `blip` and `nodes.mtx` come at the first traced solve and the second
//! step's dumps keep the numbers they have in the oracle's fully traced run
//! (`A_<n1>` onwards, `n1` the first step's iterations), byte for byte.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use akantu_comm::Communicator;
use akantu_core::debug::{self, DebugLevel};
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_linalg::matrix_market::MatrixMarketOptions;
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};

use common::drivers::upstream_compat;
use common::my_model::{Law, MyModel, gen_mesh};
use common::oracle::{assert_bits, case, i64s, input_text, param};

const CHILD: &str = "AKANTU_DOF017_DBLTRACE_CHILD";
const TEST: &str = "dof017_newton_dbltrace_matches_the_oracle";
const TEST_LATE: &str = "dof017_newton_dbltrace_counter_runs_while_untraced";

/// The solve of the oracle case in the current directory (child), traced
/// from load step `first_traced` on (`1`: the oracle's fully traced run).
fn child(first_traced: i64) {
    let f = case("dbltrace_newton_raphson");
    let n = param(&f, "nb_nodes").as_u64().unwrap() as usize;
    let df = param(&f, "load_increment").as_f64().unwrap();
    let steps = param(&f, "load_steps").as_i64().unwrap();
    let mut dm = DofManager::new("model_solver:dof_manager_default", Communicator::serial());
    dm.set_upstream_compat(upstream_compat());
    let mut model = MyModel::new(0.0, gen_mesh(n), false, &mut dm).unwrap();
    model.law = Law::Cubic;
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("static", &opts, &mut model).unwrap();
    ms.set_integration_scheme(
        "static",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &model,
    )
    .unwrap();
    {
        let nls = ms.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(10))
            .unwrap();
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
            .unwrap();
        let nr = nls.as_newton_raphson_mut().unwrap();
        nr.set_legacy_hooks(true);
        nr.set_trace_matrix_market_options(MatrixMarketOptions::upstream_precision());
    }
    let mut n_iter = Vec::new();
    let mut u = Vec::new();
    for s in 1..=steps {
        if s == first_traced {
            debug::set_debug_level(DebugLevel::TRACE);
        }
        model.forces[n - 1] = df * s as f64;
        ms.solve_step(&mut model, None).unwrap();
        n_iter.push(ms.non_linear_solver_mut(None).unwrap().nb_iterations() as i64);
        u.extend_from_slice(ms.dof_manager.dofs("disp").unwrap().as_slice());
    }
    debug::set_debug_level(DebugLevel::WARNING);
    assert_eq!(n_iter, i64s(&f, "newton/nb_iterations"), "iterations");
    assert_bits(&f, "displacement", &u);
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    v.sort();
    v
}

/// Run this binary's test `test` in a child process (environment `CHILD`
/// set) in a fresh scratch directory; returns the directory and the
/// child's standard output.
fn run_child(test: &str) -> (PathBuf, String) {
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "akantu-dof017-dbltrace-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .current_dir(&dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "child failed: {}\n{stdout}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    (dir, stdout)
}

/// Every file of `want` in `dir` equals the oracle's byte for byte; returns
/// the bytes compared.
fn assert_files_match(f: &akantu_validation::Fixture, dir: &Path, want: &[String]) -> usize {
    let mut bytes = 0;
    for name in want {
        let mine = std::fs::read(dir.join(name)).unwrap();
        let theirs = std::fs::read(f.input_path(&format!("trace:{name}")).unwrap()).unwrap();
        assert!(
            mine == theirs,
            "{name} differs from the oracle's:\n--- port\n{}\n--- oracle\n{}",
            String::from_utf8_lossy(&mine),
            String::from_utf8_lossy(&theirs)
        );
        bytes += mine.len();
    }
    bytes
}

fn oracle_trace_files(f: &akantu_validation::Fixture) -> Vec<String> {
    param(f, "trace_files")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn dof017_newton_dbltrace_matches_the_oracle() {
    if std::env::var_os(CHILD).is_some() {
        child(1);
        return;
    }
    let f = case("dbltrace_newton_raphson");
    let (dir, stdout) = run_child(TEST);
    // upstream's std::cout during the traced solves is exactly "blip\n"
    assert_eq!(input_text(&f, "stdout"), "blip\n", "oracle stdout");
    // (libtest prints `test <name> ... ` before the test's own output)
    assert_eq!(
        stdout.matches("blip\n").count(),
        1,
        "the child printed `blip` once per process: {stdout}"
    );
    let want = oracle_trace_files(&f);
    let got = files_in(&dir);
    assert_eq!(got, want, "file set");
    let bytes = assert_files_match(&f, &dir, &want);
    std::fs::remove_dir_all(&dir).unwrap();
    println!(
        "dbltrace: {} files byte-identical with the oracle ({bytes} bytes)",
        want.len()
    );
}

/// The first step untraced, the second traced. The process-wide
/// counter advanced during the untraced step's iterations, so the second
/// step's dumps are numbered from `n1` (the first step's iterations, as in
/// the oracle's fully traced run) and equal the oracle's; `first` was not
/// consumed by the untraced step: `blip` once and `nodes.mtx` (the
/// oracle's) at the first traced solve. Kills the mutants "counter
/// advanced only while tracing" (dumps from `A_0`) and "`first` consumed
/// while not tracing" (no `blip`, no `nodes.mtx`).
#[test]
fn dof017_newton_dbltrace_counter_runs_while_untraced() {
    if std::env::var_os(CHILD).is_some() {
        child(2);
        return;
    }
    let f = case("dbltrace_newton_raphson");
    let n1 = i64s(&f, "newton/nb_iterations")[0];
    assert!(n1 > 0, "the first step iterates");
    let (dir, stdout) = run_child(TEST_LATE);
    assert_eq!(
        stdout.matches("blip\n").count(),
        1,
        "`blip` once, at the first traced solve: {stdout}"
    );
    // the oracle's files without the first step's dumps (`<X>_<k>.mtx`,
    // `k < n1`)
    let want: Vec<String> = oracle_trace_files(&f)
        .into_iter()
        .filter(|name| {
            let k = name
                .strip_suffix(".mtx")
                .and_then(|s| s.split_once('_'))
                .and_then(|(_, k)| k.parse::<i64>().ok());
            k.is_none_or(|k| k >= n1)
        })
        .collect();
    assert!(want.iter().any(|n| n == "nodes.mtx"), "{want:?}");
    assert!(want.iter().any(|n| *n == format!("A_{n1}.mtx")), "{want:?}");
    let got = files_in(&dir);
    assert_eq!(got, want, "file set");
    let bytes = assert_files_match(&f, &dir, &want);
    std::fs::remove_dir_all(&dir).unwrap();
    println!(
        "dbltrace (traced from step 2): {} files byte-identical with the oracle ({bytes} bytes)",
        want.len()
    );
}
