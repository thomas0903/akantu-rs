// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_solver/test_sparse_solver_mumps.cc
// (same intent: diagonal system with every term added by the owning rank,
// b = 1, x_g = 1 + g at relative 1e-15, gathered on rank 0 — with the
// corrected gather of UD-075), plus akantu-rs acceptance scenarios of
// src/solver/sparse_solver_mumps.cc (phases, re-use, errors, workspace
// retry / UD-216, symmetry changes).

//! Real-MPI acceptance of the MUMPS backend, `harness = false`:
//! run under `mpirun -np 1/2/4` by `scripts/mumps/run-mumps-tests.sh`, or
//! as an MPI singleton by `cargo test --features mumps`. Every scenario
//! runs on every rank in the same order; outcomes are agreed on a control
//! communicator; the process has a watchdog.
//!
//! Environment: `AKANTU_MUMPS_THREAD_LEVEL` (`funneled` default,
//! `serialized`, `multiple`); `AKANTU_MUMPS_EXPORT=<dir>` writes every
//! system (per-rank AIJ files) and the gathered solutions for the oracle
//! comparison (`oracle/probes/mumps/`); at 1 rank the solutions are compared
//! bit for bit with the committed oracle solutions of **this platform and
//! OpenBLAS pin** (D-043, D-050): x86_64 →
//! `fixtures/solver/mumps/x86_64-haswell/` (requires `OPENBLAS_CORETYPE =
//! Haswell`, `OPENBLAS_NUM_THREADS = 1`, checked by the `blas_pin`
//! scenario), aarch64 → `fixtures/solver/mumps/` (the aarch64 platform
//! variant, OpenBLAS's run-time core); the other platform's set is compared
//! as a diagnostic only. `AKANTU_MUMPS_ORACLE=<dir>` overrides the
//! authoritative set, `off` disables the comparison (export runs).

mod mumps_common;

use akantu_validation::reduce::NanMinMax;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, MpiOptions, ReduceOp};
use akantu_core::enums::SparseSolverType;
use akantu_core::{Error, Real, Result};
use akantu_linalg::SparseMatrixAij;
use akantu_linalg::centralize::gather_owned_to_root;
use akantu_linalg::direct::{MatrixDistribution, SparseDirectSolver, SparseSolverFactory};
use akantu_linalg::mumps::{MumpsOptions, MumpsSolver, RetryRhs, blas_info};

use mumps_common::*;

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

fn level() -> ThreadLevel {
    match std::env::var("AKANTU_MUMPS_THREAD_LEVEL").as_deref() {
        Ok("serialized") => ThreadLevel::Serialized,
        Ok("multiple") => ThreadLevel::Multiple,
        _ => ThreadLevel::Funneled,
    }
}

fn world() -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: level(),
        timeout: Some(Duration::from_secs(600)),
        ..MpiOptions::default()
    })
    .expect("MPI world")
}

fn export_dir() -> Option<PathBuf> {
    std::env::var_os("AKANTU_MUMPS_EXPORT").map(PathBuf::from)
}

/// One committed set of 1-rank oracle solutions (upstream
/// `SparseSolverMumps` on the same exported systems, oracle/probes/mumps):
/// bitwise comparable only on its platform with its OpenBLAS pin (D-050).
#[derive(Clone, Debug)]
struct FixtureSet {
    label: &'static str,
    dir: PathBuf,
    /// `OPENBLAS_CORETYPE` the set was generated with (`None`: unset, the
    /// run-time selection).
    coretype: Option<&'static str>,
}

/// The fixture sets: `(authoritative for this platform, diagnostic ones)`.
/// `AKANTU_MUMPS_ORACLE=<dir>` replaces the authoritative set (no pin
/// requirement), `=off` disables every comparison.
fn oracle_sets() -> (Option<FixtureSet>, Vec<FixtureSet>) {
    let root = akantu_validation::fixtures_root().join("solver/mumps");
    let x86 = FixtureSet {
        label: "x86_64, OpenBLAS core Haswell, 1 thread (D-050)",
        dir: root.join("x86_64-haswell"),
        coretype: Some("Haswell"),
    };
    let arm = FixtureSet {
        label: "aarch64 platform variant, OpenBLAS run-time core, 1 thread",
        dir: root,
        coretype: None,
    };
    match std::env::var_os("AKANTU_MUMPS_ORACLE") {
        Some(v) if v == "off" => (None, Vec::new()),
        Some(v) => (
            Some(FixtureSet {
                label: "AKANTU_MUMPS_ORACLE override",
                dir: PathBuf::from(v),
                coretype: None,
            }),
            Vec::new(),
        ),
        None => match std::env::consts::ARCH {
            "x86_64" => (Some(x86), vec![arm]),
            "aarch64" => (Some(arm), vec![x86]),
            _ => (None, vec![x86, arm]),
        },
    }
}

/// The OpenBLAS pin of `set` is in force (D-050): `OPENBLAS_NUM_THREADS =
/// 1`, `OPENBLAS_CORETYPE` as recorded (unset for a run-time-core set) and,
/// for a pinned set, the core OpenBLAS actually uses. `Err` describes the
/// difference (absent or different pin).
fn pin_check(set: &FixtureSet) -> std::result::Result<(), String> {
    if set.label.contains("override") {
        return Ok(());
    }
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let blas = blas_info();
    let threads = env("OPENBLAS_NUM_THREADS");
    if threads.as_deref() != Some("1") || blas.threads.is_some_and(|t| t != 1) {
        return Err(format!(
            "OPENBLAS_NUM_THREADS = {threads:?} (OpenBLAS reports {:?} threads); the fixture set \
             '{}' needs 1",
            blas.threads, set.label
        ));
    }
    let core = env("OPENBLAS_CORETYPE");
    if core.as_deref() != set.coretype {
        return Err(format!(
            "OPENBLAS_CORETYPE = {core:?}, the fixture set '{}' was generated with {:?} (D-050 \
             BLAS pin absent or different)",
            set.label, set.coretype
        ));
    }
    if let Some(want) = set.coretype
        && !blas
            .core
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(want))
    {
        return Err(format!(
            "OpenBLAS runs core {:?} although OPENBLAS_CORETYPE = {want} (fixture set '{}')",
            blas.core, set.label
        ));
    }
    Ok(())
}

/// The BLAS pin required by this platform's authoritative fixture set is in
/// force (D-050): fails when it is absent or different. Prints
/// the BLAS provenance.
fn blas_pin(c: &Communicator) -> Outcome {
    let blas = blas_info();
    let (auth, _) = oracle_sets();
    if c.rank() == 0 {
        println!(
            "INFO blas: platform={} OPENBLAS_CORETYPE={:?} OPENBLAS_NUM_THREADS={:?} \
             openblas_core={:?} openblas_threads={:?} authoritative_fixtures={:?}",
            std::env::consts::ARCH,
            std::env::var("OPENBLAS_CORETYPE").ok(),
            std::env::var("OPENBLAS_NUM_THREADS").ok(),
            blas.core,
            blas.threads,
            auth.as_ref().map(|s| s.label)
        );
    }
    match &auth {
        Some(set) => pin_check(set),
        None => Ok(()),
    }
}

/// Solve `sys` with `solver` on `comm`: rank-partial matrix, layout with
/// shared/ghost equations; returns `(gathered x on rank 0, local solution)`.
fn solve(
    comm: &Communicator,
    solver: &mut dyn SparseDirectSolver,
    sys: &System,
    a: &SparseMatrixAij,
) -> Result<(Option<Vec<Real>>, Vec<Real>)> {
    let (r, p) = (comm.rank() as usize, comm.size());
    let layout = layout(sys.n, p, r);
    let rhs = local_rhs(&layout, &sys.b);
    let mut x = vec![-7.0; layout.nb_local()];
    let mut ph = Phases::new(comm, "mumps-test");
    solver.solve_distributed(&mut ph, "t", &layout, Ok(a), &rhs, &mut x)?;
    // shared rows untouched (the DOF manager synchronises them), pure ghost
    // untouched
    for (l, &v) in x.iter().enumerate() {
        if !layout.is_owned(l) && v != -7.0 {
            return Err(Error::invalid(format!("non-owned local row {l} written")));
        }
    }
    let g = gather_owned_to_root(&mut ph, "gather", &layout, &x)?;
    Ok((g, x))
}

fn bitwise(a: &[Real], b: &[Real]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

fn max_rel_diff(a: &[Real], b: &[Real]) -> Real {
    let scale = b
        .iter()
        .fold(0.0_f64, |m, v| m.nan_max(v.abs()))
        .max(f64::MIN_POSITIVE);
    a.iter()
        .zip(b)
        .fold(0.0_f64, |m, (x, y)| m.nan_max((x - y).abs()))
        / scale
}

/// Checks of rank-0-only data (gathered solutions) are **deferred** to the
/// end of a scenario: a rank that left a scenario early while its peers
/// continue into the next collective would hang them (each scenario's
/// collectives must run to completion on every rank).
#[derive(Default)]
struct Deferred(Vec<String>);

impl Deferred {
    fn check(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.0.push(msg());
        }
    }

    fn finish(self) -> Outcome {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.0.join("; "))
        }
    }
}

// ------------------------------------------------------------- scenarios

/// `test_sparse_solver_mumps` (UD-075 corrected gather): x_g = 1 + g to
/// relative 1e-15 (upstream's tolerance; MUMPS divides by the diagonal).
fn upstream_test_sparse_solver_mumps(c: &Communicator) -> Outcome {
    let sys = upstream_diagonal(c.size());
    let a = sys.matrix(c.rank() as usize);
    let mut s = MumpsSolver::default();
    ensure!(
        s.parallel_method() == akantu_linalg::direct::SolverParallelMethod::FullyDistributed,
        "parallel method"
    );
    let (g, _) = solve(c, &mut s, &sys, &a).map_err(|e| e.to_string())?;
    if let Some(x) = g {
        for (k, &v) in x.iter().enumerate() {
            let d = 1.0 + k as Real;
            ensure!(
                ((v - d) / d).abs() <= 1e-15,
                "Error in the solution: {v} != {d}"
            );
        }
        let rep = s.report();
        println!(
            "INFO upstream_test_sparse_solver_mumps: MUMPS {} sym={} ranks={} jobs={:?}",
            rep.version, rep.sym, rep.nb_proc, rep.jobs
        );
    }
    Ok(())
}

/// Systems of the oracle comparison: Laplacians (sym/unsym), the
/// "SMM-like" elasticity system and the assembled AIJ systems of the solver
/// LU fixtures (`fixtures/solver/lu`, unsymmetric).
fn comparison_systems(p: usize) -> Vec<System> {
    let mut v = vec![
        upstream_diagonal(p),
        laplacian(6, true, p),
        laplacian(6, false, p),
        laplacian(12, true, p),
        elasticity(4, p),
        elasticity(6, p),
        elasticity(8, p),
    ];
    for name in [
        "arrow",
        "convection_diffusion",
        "dense_block",
        "diag_upstream_test",
        "explicit_zeros_duplicates",
        "random_pivoting",
    ] {
        v.push(lu_fixture(name, p));
    }
    v
}

/// An assembled AIJ system of `fixtures/solver/lu/<name>` (triplets in
/// `setFromTriplets` order added to an unsymmetric AIJ matrix, contiguous
/// triplet blocks per rank).
fn lu_fixture(name: &str, p: usize) -> System {
    let dir = akantu_validation::fixtures_root().join(format!("solver/lu/{name}/data"));
    let rd = |f: &str| akantu_validation::npy::read_npy(dir.join(f)).expect("npy");
    let rows = rd("A__rows.npy").to_i64().expect("i64");
    let cols = rd("A__cols.npy").to_i64().expect("i64");
    let vals = rd("A__values.npy").to_f64().expect("f64");
    let b = rd("b.npy").to_f64().expect("f64");
    let all: Vec<_> = (0..rows.len())
        .map(|k| (rows[k], cols[k], vals[k]))
        .collect();
    System {
        name: format!("lu_{name}"),
        n: b.len(),
        sym: false,
        per_rank: split(&all, p),
        b,
    }
}

/// Every comparison system: residual check, optional export, optional
/// bitwise check against committed oracle solutions (1 rank).
fn oracle_systems(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let export_to = export_dir();
    let (auth, diagnostic) = oracle_sets();
    let pinned = auth.as_ref().map(pin_check);
    if let (Some(Err(e)), 0) = (&pinned, r) {
        println!("INFO oracle comparison: {e}");
    }
    let tag = std::env::var("AKANTU_MUMPS_TAG").unwrap_or_else(|_| "rust".into());
    let mut d0 = Deferred::default();
    for sys in comparison_systems(p) {
        let a = sys.matrix(r);
        if let Some(d) = &export_to {
            export(d, &sys, p, r, &a);
        }
        let mut s = MumpsSolver::default();
        // solve errors are agreed (identical on every rank): leaving here is safe
        let (g, _) = solve(c, &mut s, &sys, &a).map_err(|e| format!("{}: {e}", sys.name))?;
        if let Some(x) = g {
            let res = sys.residual(&x);
            let (amplification, backward) = sys.backward_error(&x);
            let rep = s.report();
            println!(
                "INFO system {} n={} sym={} ranks={p} residual={res:.3e} ordering(INFOG(7))={} \
                 icntl14={} retries={} normA_F*normx/normb={amplification:.3e} \
                 backward_error={backward:.3e}",
                sys.name,
                sys.n,
                u8::from(sys.sym),
                rep.infog(7),
                rep.icntl14,
                rep.retries
            );
            d0.check(res < 1e-10, || {
                format!("{}: relative residual {res:e}", sys.name)
            });
            if let Some(d) = &export_to {
                export_solution(d, &sys.name, p, &tag, &x);
            }
            if let (Some(set), 1) = (&auth, p) {
                let f = set.dir.join(format!("{}.p1.oracle.x.txt", sys.name));
                match (&pinned, read_solution(&f)) {
                    (Some(Err(e)), _) => d0.check(false, || {
                        format!("{}: not compared with '{}': {e}", sys.name, set.label)
                    }),
                    (_, None) => d0.check(false, || format!("missing {}", f.display())),
                    (_, Some(want)) => {
                        let same = bitwise(&x, &want);
                        println!(
                            "INFO oracle {} p=1 [{}] bitwise={same} max_rel_diff={:.3e}",
                            sys.name,
                            set.label,
                            max_rel_diff(&x, &want)
                        );
                        d0.check(same, || {
                            format!(
                                "{}: 1-rank solution differs from the oracle's ({})",
                                sys.name, set.label
                            )
                        });
                    }
                }
            }
            if p == 1 {
                for set in &diagnostic {
                    let f = set.dir.join(format!("{}.p1.oracle.x.txt", sys.name));
                    if let Some(want) = read_solution(&f) {
                        println!(
                            "INFO diagnostic oracle {} p=1 [{}] bitwise={} max_rel_diff={:.3e}",
                            sys.name,
                            set.label,
                            bitwise(&x, &want),
                            max_rel_diff(&x, &want)
                        );
                    }
                }
            }
        }
    }
    d0.finish()
}

/// Re-use: unchanged system → solve only; new values → factorisation +
/// solve; new profile → analysis + factorisation + solve; symmetry change
/// (UD-034 promotion) → re-initialisation.
fn phases_and_reuse(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let sys = laplacian(5, true, p);
    let mut a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    let e2s = |e: Error| e.to_string();
    // `report().jobs` is identical on every rank (agreed plan, global
    // INFOG): those checks may return at once; gathered solutions are
    // checked at the end (rank 0).
    let mut d0 = Deferred::default();
    solve(c, &mut s, &sys, &a).map_err(e2s)?;
    ensure!(
        s.report().jobs == [-1, 1, 2, 3],
        "first solve {:?}",
        s.report().jobs
    );
    let (x0, _) = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    ensure!(s.report().jobs == [3], "unchanged {:?}", s.report().jobs);
    // values only (one rank changes): every rank refactorises (agreed)
    if r == p - 1 {
        // value release bump on an existing entry, same operator
        let (i, j, _) = a.coo();
        let (i0, j0) = (i[0], j[0]);
        a.add(i0, j0, 0.0).map_err(e2s)?;
    }
    let (x1, _) = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    ensure!(s.report().jobs == [2, 3], "values {:?}", s.report().jobs);
    if let (Some(x0), Some(x1)) = (&x0, &x1) {
        // 1 rank: bitwise (same factorisation); several ranks: MUMPS's
        // parallel factorisation is not reproducible run to run
        // (ORACLE_BUILD) — measured, bounded by the conditioning
        let same = bitwise(x0, x1);
        println!(
            "INFO refactorisation of identical values: ranks={p} bitwise={same} max_rel_diff={:.3e}",
            max_rel_diff(x1, x0)
        );
        d0.check(same || (p > 1 && max_rel_diff(x1, x0) < 1e-12), || {
            format!("refactorisation changed x by {:.3e}", max_rel_diff(x1, x0))
        });
    }
    // profile change on one rank only (new zero entry)
    if r == 0 {
        a.add(0, sys.n as i64 - 1, 0.0).map_err(e2s)?;
    }
    solve(c, &mut s, &sys, &a).map_err(e2s)?;
    ensure!(
        s.report().jobs == [1, 2, 3],
        "profile {:?}",
        s.report().jobs
    );
    // promotion to unsymmetric: sym 2 -> 0, re-initialisation
    a.promote_to_unsymmetric();
    let mut sys_u = sys.clone();
    sys_u.sym = false;
    // the promoted storage holds both triangles: the global operator is
    // unchanged, so is the solution (up to rounding)
    let (x2, _) = solve(c, &mut s, &sys_u, &a).map_err(e2s)?;
    ensure!(
        s.report().jobs == [-2, -1, 1, 2, 3] && s.report().sym == 0,
        "promotion {:?} sym {}",
        s.report().jobs,
        s.report().sym
    );
    if let (Some(x0), Some(x2)) = (&x0, &x2) {
        d0.check(max_rel_diff(x2, x0) < 1e-12, || {
            "promoted solve differs".into()
        });
    }
    s.destroy().map_err(e2s)?;
    solve(c, &mut s, &sys_u, &a).map_err(e2s)?;
    ensure!(
        s.report().jobs == [-1, 1, 2, 3],
        "after destroy {:?}",
        s.report().jobs
    );
    d0.finish()
}

/// SOLV-003: singular matrices → `Error::LinearSolver` on every rank,
/// non-finite input → `Error::Numerical` (before MUMPS), mismatched shapes
/// → agreed refusal; the solver stays usable afterwards.
fn errors_are_agreed(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    // numerically singular (rows 0 and 3 identical) and structurally
    // singular (empty column 2) fixtures
    for name in ["numerically_singular", "structurally_singular"] {
        let sys = lu_fixture(name, p);
        let a = sys.matrix(r);
        let mut s = MumpsSolver::default();
        let t0 = Instant::now();
        let res = solve(c, &mut s, &sys, &a);
        let ok = matches!(&res, Err(Error::LinearSolver(m)) if m.contains("singular"));
        println!(
            "INFO {name}: {} (INFOG(1) = {}, {:?})",
            res.as_ref()
                .map_or_else(ToString::to_string, |_| "solved".into()),
            s.report().infog(1),
            t0.elapsed()
        );
        ensure!(ok, "{name}: expected a singular-matrix error, got {res:?}");
        // still usable on a regular system
        let reg = laplacian(4, false, p);
        solve(c, &mut s, &reg, &reg.matrix(r)).map_err(|e| format!("{name} then regular: {e}"))?;
    }
    // non-finite entry on the last rank only
    let sys = laplacian(4, true, p);
    let mut a = sys.matrix(r);
    if r == p - 1 {
        a.add(1, 1, f64::NAN).map_err(|e| e.to_string())?;
    }
    let mut s = MumpsSolver::default();
    let res = solve(c, &mut s, &sys, &a);
    if r == p - 1 {
        ensure!(matches!(res, Err(Error::Numerical(_))), "NaN rank: {res:?}");
    } else {
        ensure!(res.is_err(), "NaN on another rank: {res:?}");
    }
    ensure!(s.report().jobs.is_empty(), "MUMPS ran on non-finite input");
    // a wrong local vector length on rank 0 only
    let a = sys.matrix(r);
    let lay = layout(sys.n, p, r);
    let mut rhs = local_rhs(&lay, &sys.b);
    if r == 0 {
        rhs.pop();
    }
    let mut x = vec![0.0; lay.nb_local()];
    let mut ph = Phases::new(c, "bad");
    let res = s.solve_distributed(&mut ph, "t", &lay, Ok(&a), &rhs, &mut x);
    ensure!(res.is_err(), "bad length accepted: {res:?}");
    drop(ph);
    let (g, _) = solve(c, &mut s, &sys, &a).map_err(|e| e.to_string())?;
    if let Some(x) = g {
        ensure!(sys.residual(&x) < 1e-10, "after errors: residual");
    }
    Ok(())
}

/// A user ICNTL value that makes MUMPS read or write
/// a user array the backend never provides (ICNTL(7) = 1, (8) = −1,
/// (15) > 0, (19), (25) < 0, (26), (30), (32) ≠ 0) is refused on every rank
/// before MUMPS runs — also when only rank 0 sets it — and the solver stays
/// usable; values that need no user array (iterative refinement, error
/// analysis, null-pivot detection) still solve.
fn ua04_array_backed_icntl(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let sys = laplacian(4, false, p);
    let a = sys.matrix(r);
    for (i, v) in [
        (7, 1),
        (8, -1),
        (15, 1),
        (19, 1),
        (25, -1),
        (26, 1),
        (30, 1),
        (32, 1),
    ] {
        for only_rank0 in [false, true] {
            let mut opts = MumpsOptions::default();
            if !only_rank0 || r == 0 {
                opts.icntl.insert(i, v);
            }
            let mut s = MumpsSolver::with_options(opts);
            let res = solve(c, &mut s, &sys, &a);
            ensure!(
                res.is_err(),
                "ICNTL({i}) = {v} (rank 0 only: {only_rank0}) accepted on rank {r}"
            );
            if !only_rank0 || r == 0 {
                ensure!(
                    matches!(&res, Err(Error::Unsupported(m)) if m.contains("the backend does not provide")),
                    "ICNTL({i}) = {v}: {res:?}"
                );
            }
            ensure!(
                s.report().jobs.is_empty(),
                "ICNTL({i}) = {v}: MUMPS ran ({:?})",
                s.report().jobs
            );
            s.options_mut().icntl.remove(&i);
            let (g, _) =
                solve(c, &mut s, &sys, &a).map_err(|e| format!("after ICNTL({i}): {e}"))?;
            if let Some(x) = g {
                ensure!(sys.residual(&x) < 1e-10, "after ICNTL({i}) = {v}: residual");
            }
        }
    }
    for (i, v) in [(10, 2), (11, 2), (24, 1)] {
        let mut opts = MumpsOptions::default();
        opts.icntl.insert(i, v);
        let mut s = MumpsSolver::with_options(opts);
        let (g, _) = solve(c, &mut s, &sys, &a).map_err(|e| format!("ICNTL({i}) = {v}: {e}"))?;
        if let Some(x) = g {
            ensure!(sys.residual(&x) < 1e-10, "ICNTL({i}) = {v}: residual");
        }
    }
    Ok(())
}

/// UD-216 regression: a workspace retry forced by a negative `ICNTL(14)`
/// returns the solution of the **original** right-hand side (bitwise equal
/// to a solve without retry: the workspace size does not change the
/// arithmetic), while the upstream-compatible mode returns `A⁻²b` after one
/// factorisation retry (`A^-(k+1) b` after k), as upstream's recursion.
fn ud216_workspace_retry(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let sys = elasticity(6, p);
    let a = sys.matrix(r);
    let e2s = |e: Error| e.to_string();
    let mut plain = MumpsSolver::default();
    let (x_ref, _) = solve(c, &mut plain, &sys, &a).map_err(e2s)?;
    ensure!(plain.report().retries == 0, "reference solve retried");
    let mut opts = MumpsOptions::default();
    opts.icntl.insert(14, -80);
    let mut retried = MumpsSolver::with_options(opts.clone());
    let (x, _) = solve(c, &mut retried, &sys, &a).map_err(e2s)?;
    let rep = retried.report().clone();
    ensure!(
        rep.factorization_retries >= 1,
        "no factorisation retry was forced (ICNTL(14) = -80): {rep:?}"
    );
    // second solve with the same factors: no retry, same ICNTL(14) kept
    let (x_again, _) = solve(c, &mut retried, &sys, &a).map_err(e2s)?;
    ensure!(
        retried.report().jobs == [3] && retried.report().icntl14 == rep.icntl14,
        "after retry {:?}",
        retried.report()
    );
    // compat: upstream's wrong solution
    opts.retry_rhs = RetryRhs::UpstreamSolution;
    let mut compat = MumpsSolver::with_options(opts);
    let (xc, _) = solve(c, &mut compat, &sys, &a).map_err(e2s)?;
    // INFOG is global: every rank saw the same number of retries
    let k = compat.report().factorization_retries;
    ensure!(k >= 1, "compat: no factorisation retry");
    // A^-(k+1) b by k + 1 plain solves (rank 0's iterate broadcast as the
    // next global right-hand side)
    let mut want = x_ref.clone().unwrap_or_default();
    for _ in 0..k {
        let mut buf = if r == 0 {
            want.clone()
        } else {
            vec![0.0; sys.n]
        };
        c.broadcast(&mut buf, 0).map_err(e2s)?;
        let mut s2 = sys.clone();
        s2.b = buf;
        let (y, _) = solve(c, &mut plain, &s2, &a).map_err(e2s)?;
        want = y.unwrap_or_default();
    }
    if let (Some(x_ref), Some(x), Some(x_again), Some(xc)) = (&x_ref, &x, &x_again, &xc) {
        if let Some(d) = export_dir() {
            // oracle comparison of the retry path (oracle/probes/mumps with
            // the ICNTL(14) interposer): corrected and upstream-compatible
            let tag = std::env::var("AKANTU_MUMPS_TAG").unwrap_or_else(|_| "rust".into());
            export_solution(&d, &sys.name, p, &format!("{tag}-retry"), x);
            export_solution(&d, &sys.name, p, &format!("{tag}-compat"), xc);
        }
        let res = sys.residual(x);
        let same = bitwise(x, x_ref);
        println!(
            "INFO ud216: ranks={p} retries={} (factorisation {}) final ICNTL(14)={} \
             residual={res:.3e} bitwise_to_no_retry={same} max_rel_diff={:.3e}",
            rep.retries,
            rep.factorization_retries,
            rep.icntl14,
            max_rel_diff(x, x_ref)
        );
        ensure!(res < 1e-10, "retried solve: residual {res:e}");
        // same factors: bitwise at 1 rank; MUMPS's parallel solve phase is
        // not reproducible run to run on several ranks (measured)
        ensure!(
            bitwise(x, x_again) || (p > 1 && max_rel_diff(x_again, x) < 1e-12),
            "re-solve with the retried factors differs by {:.3e}",
            max_rel_diff(x_again, x)
        );
        // 1 rank: bitwise against the oracle's runs of the same system
        // (upstream SparseSolverMumps; its retry path forced by the
        // ICNTL(14) interposer with the same start, -80)
        let (auth, diagnostic) = oracle_sets();
        if p == 1 {
            let cmp = |set: &FixtureSet| {
                let read =
                    |t: &str| read_solution(&set.dir.join(format!("{}.p1.{t}.x.txt", sys.name)));
                let (o, o_retry) = (read("oracle"), read("oracle-retry"));
                o.as_ref().is_some_and(|o| bitwise(x, o))
                    && o_retry.as_ref().is_some_and(|o| bitwise(xc, o))
            };
            for set in &diagnostic {
                println!(
                    "INFO diagnostic ud216 oracle p=1 [{}]: bitwise: {}",
                    set.label,
                    cmp(set)
                );
            }
            if let Some(set) = &auth {
                pin_check(set).map_err(|e| format!("UD-216 oracle comparison: {e}"))?;
                let ok = cmp(set);
                println!(
                    "INFO ud216 oracle p=1 [{}]: corrected retry == oracle without retry and \
                     compat == oracle's retry path, bitwise: {ok}",
                    set.label
                );
                ensure!(ok, "UD-216 oracle comparison failed ({})", set.label);
            }
        }
        let rel = max_rel_diff(xc, x_ref);
        println!(
            "INFO ud216 compat: k={k} max|x_compat - x|/max|x| = {rel:.3e} (upstream's wrong \
             solution), bitwise to A^-(k+1) b: {}",
            bitwise(xc, &want)
        );
        ensure!(rel > 1e-3, "compat mode did not reproduce UD-216");
        ensure!(
            max_rel_diff(xc, &want) < 1e-12,
            "compat result is not A^-(k+1) b (max rel {:.3e})",
            max_rel_diff(xc, &want)
        );
    }
    Ok(())
}

/// The workspace ceiling: a system whose factorisation keeps failing with
/// `-9` ends with upstream's "workarray is too small" error once
/// `ICNTL(14)` reaches the ceiling, on every rank, and the solver stays
/// usable. Upstream's ceiling is 90 (unit test `workspace_retry_rule`); no
/// system is known to fail at every `ICNTL(14) < 90`, so the mechanism is
/// exercised with a lowered ceiling: start −80 on the elasticity system
/// (first success at 0, see `ud216_workspace_retry`), ceiling −50 → tries
/// at −80, −70, −60, error when −50 is reached.
fn workspace_ceiling(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let sys = elasticity(6, p);
    let a = sys.matrix(r);
    let mut opts = MumpsOptions::default();
    opts.icntl.insert(14, -80);
    opts.icntl14_ceiling = Some(-50);
    let mut s = MumpsSolver::with_options(opts);
    let res = solve(c, &mut s, &sys, &a);
    let rep = s.report().clone();
    if r == 0 {
        println!(
            "INFO workspace ceiling: {} retries, final ICNTL(14)={}, {:?}",
            rep.retries,
            rep.icntl14,
            res.as_ref().err()
        );
    }
    ensure!(
        matches!(&res, Err(Error::Backend { code: -9, message, .. }) if message.contains("too small")),
        "expected the workspace error, got {:?}",
        res.as_ref().err()
    );
    ensure!(
        rep.icntl14 == -50 && rep.retries == 3,
        "stopped at {} after {}",
        rep.icntl14,
        rep.retries
    );
    // usable afterwards (a later solve redoes analysis and factorisation;
    // the increased ICNTL(14) is kept, as upstream keeps it)
    s.options_mut().icntl14_ceiling = None;
    let (g, _) = solve(c, &mut s, &sys, &a).map_err(|e| e.to_string())?;
    if let Some(x) = g {
        ensure!(sys.residual(&x) < 1e-10, "after the ceiling: residual");
    }
    Ok(())
}

/// The factory: `mumps` registered; `auto` → MUMPS on an MPI communicator,
/// Eigen elsewhere; `create(mumps, nb_proc)` allowed on several processes,
/// `create_centralized(mumps)` refused (not a centralised backend).
fn factory_and_availability(c: &Communicator) -> Outcome {
    let f = SparseSolverFactory::default();
    ensure!(f.is_registered("mumps"), "mumps not registered");
    ensure!(
        f.resolve_available(SparseSolverType::Auto, c.is_mpi()) == SparseSolverType::Mumps,
        "auto on MPI"
    );
    ensure!(
        f.resolve_available(SparseSolverType::Auto, false) == SparseSolverType::Eigen,
        "auto off MPI"
    );
    let s = f
        .create(SparseSolverType::Mumps, c.size().max(4))
        .map_err(|e| e.to_string())?;
    ensure!(
        s.kind() == SparseSolverType::Mumps
            && s.capabilities().matrix == MatrixDistribution::DistributedAssembled,
        "caps"
    );
    ensure!(
        f.create_centralized(SparseSolverType::Mumps).is_err(),
        "centralised MUMPS accepted"
    );
    ensure!(
        MumpsSolver::mumps_int_bytes() == 4,
        "MUMPS_INT is not 32-bit"
    );
    // a serial communicator: refused (no MPI handle), nothing hangs
    let serial = Communicator::serial();
    let sys = upstream_diagonal(1);
    let mut s = MumpsSolver::default();
    let res = solve(&serial, &mut s, &sys, &sys.matrix(0));
    ensure!(matches!(res, Err(Error::Unsupported(_))), "serial: {res:?}");
    Ok(())
}

// The `Serialized` admission interleavings (a pending duplicate / split /
// native collective on another thread against a first initialisation, a
// re-initialisation and a destruction, with agreed outcomes, retries and
// instance accounting) are `review_v54_regressions.rs`
// (`mu04_refused_reinit`, `mu05_admission_interleavings`); they replace
// the former `serialized_admission` scenario, which created no contention.

fn main() {
    let watchdog = std::env::var("AKANTU_MUMPS_WATCHDOG_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(900u64);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(watchdog));
        eprintln!("mumps_mpi: watchdog ({watchdog} s) expired");
        std::process::exit(70);
    });
    let control = world();
    let work = world();
    let rank = control.rank();
    if rank == 0 {
        println!(
            "INFO mumps_mpi: ranks={} thread_level={:?}",
            control.size(),
            level()
        );
    }
    let scenarios: [Scenario; 9] = [
        ("factory_and_availability", factory_and_availability),
        ("blas_pin", blas_pin),
        (
            "upstream_test_sparse_solver_mumps",
            upstream_test_sparse_solver_mumps,
        ),
        ("oracle_systems", oracle_systems),
        ("phases_and_reuse", phases_and_reuse),
        ("errors_are_agreed", errors_are_agreed),
        ("ud216_workspace_retry", ud216_workspace_retry),
        ("workspace_ceiling", workspace_ceiling),
        ("ua04_array_backed_icntl", ua04_array_backed_icntl),
    ];
    let only = std::env::var("AKANTU_MUMPS_ONLY").ok();
    let mut failed = 0;
    for (name, f) in scenarios {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == name))
        {
            continue;
        }
        let t0 = Instant::now();
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&work)))
            .unwrap_or_else(|_| Err("panicked".into()));
        let mut ok = [u8::from(out.is_ok())];
        control
            .all_reduce(&mut ok, ReduceOp::Min)
            .expect("control reduction");
        let verdict = if ok[0] == 1 { "PASS" } else { "FAIL" };
        if ok[0] == 0 {
            failed += 1;
        }
        if let Err(e) = &out {
            println!("rank {rank}: {name}: {e}");
        }
        if rank == 0 {
            println!("{verdict} {name} ({:.2} s)", t0.elapsed().as_secs_f64());
        }
    }
    if rank == 0 {
        println!("mumps_mpi: failed={failed}");
    }
    drop(work);
    drop(control);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
