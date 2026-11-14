// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression scenarios of the MUMPS
// backend ported from Akantu (EPFL/LSMS) src/solver/sparse_solver_mumps.cc
// (initialize, initMumpsData, analysis/factorize/solve, printError,
// mumpsDataDestroy): communicator binding, agreement of the
// options and of every control-flow decision, ownership of the old
// instance across a refused re-initialisation and controlled
// `Serialized` admission interleavings.

//! Real-MPI regressions (`harness = false`), one process per
//! scenario or every scenario in order:
//!
//! ```text
//! mpirun -n N review_v54_regressions [scenario]
//! AKANTU_MUMPS_THREAD_LEVEL=serialized mpirun -n N review_v54_regressions mu04_refused_reinit
//! ```
//!
//! Every scenario runs its collectives to completion on every rank (rank-0
//! data is checked at the end); the verdict is agreed on a control
//! communicator; a watchdog bounds the process (a scenario that hangs on a
//! defective base fails by the watchdog or the runner's `timeout`). The
//! admission scenarios need `MPI_THREAD_SERIALIZED`; at the
//! other levels they report "not applicable" and pass.

mod mumps_common;

use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, MpiOptions, ReduceOp};
use akantu_core::{Error, Real, Result};
use akantu_linalg::SparseMatrixAij;
use akantu_linalg::centralize::gather_owned_to_root;
use akantu_linalg::direct::SparseDirectSolver;
use akantu_linalg::mumps::{InstanceCounts, MumpsSolver, RetryRhs, instance_counts};

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
        timeout: Some(Duration::from_secs(120)),
        ..MpiOptions::default()
    })
    .expect("MPI world")
}

/// Solve `sys` on `comm`; returns `(gathered x on rank 0, local x)`.
fn solve(
    comm: &Communicator,
    solver: &mut MumpsSolver,
    sys: &System,
    a: &SparseMatrixAij,
) -> Result<(Option<Vec<Real>>, Vec<Real>)> {
    let (r, p) = (comm.rank() as usize, comm.size());
    let layout = layout(sys.n, p, r);
    let rhs = local_rhs(&layout, &sys.b);
    let mut x = vec![-7.0; layout.nb_local()];
    let mut ph = Phases::new(comm, "v54");
    solver.solve_distributed(&mut ph, "t", &layout, Ok(a), &rhs, &mut x)?;
    let g = gather_owned_to_root(&mut ph, "gather", &layout, &x)?;
    Ok((g, x))
}

/// The diagonal system's exact solution `x_g = 1 + g` (relative 1e-15,
/// upstream's tolerance), checked where the gathered vector is.
fn diagonal_ok(g: &Option<Vec<Real>>) -> std::result::Result<(), String> {
    if let Some(x) = g {
        for (k, &v) in x.iter().enumerate() {
            let d = 1.0 + k as Real;
            if ((v - d) / d).abs() > 1e-15 {
                return Err(format!("x[{k}] = {v}, expected {d}"));
            }
        }
    }
    Ok(())
}

/// Rank-0-only checks deferred to the end of a scenario.
#[derive(Default)]
struct Deferred(Vec<String>);

impl Deferred {
    fn check(&mut self, r: std::result::Result<(), String>) {
        if let Err(e) = r {
            self.0.push(e);
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

fn e2s(e: Error) -> String {
    e.to_string()
}

/// No native job ran between `before` and now on this rank.
fn no_native_job(before: InstanceCounts, what: &str) -> Outcome {
    let now = instance_counts();
    ensure!(
        now.jobs == before.jobs,
        "{what}: {} native MUMPS job(s) ran before the refusal",
        now.jobs - before.jobs
    );
    Ok(())
}

/// Every successful native initialisation since `before` has its native
/// destruction, and every allocated instance was freed.
fn balanced(before: InstanceCounts, what: &str) -> Outcome {
    let now = instance_counts();
    let inits = now.initialized - before.initialized;
    let destroys = now.destroyed - before.destroyed;
    let allocs = now.allocated - before.allocated;
    let frees = now.freed - before.freed;
    println!(
        "INFO {what}: native initialisations {inits}, destructions {destroys}, allocations \
         {allocs}, frees {frees}"
    );
    ensure!(
        inits == destroys,
        "{what}: {inits} native initialisations (JOB = -1) but {destroys} native destructions \
         (JOB = -2): MUMPS instances orphaned"
    );
    ensure!(
        allocs == frees,
        "{what}: {allocs} instances allocated, {frees} freed"
    );
    Ok(())
}

// ------------------------------------------------------------------

/// An instance bound to `c` is reused on a **reordered**
/// communicator (`split` with reversed keys). Upstream-shaped reuse would
/// run the instance's original rank order while the RHS gather/scatter use
/// the new one (every owned value became 1). Required: an
/// agreed refusal before any native job, the solver still usable on its
/// communicator, and the explicit rebinding protocol (`destroy()` on the
/// old communicator, then a solve on the new one) giving `x_g = 1 + g`.
fn mu01_reordered_communicator(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let sys = upstream_diagonal(p);
    let a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    let mut d0 = Deferred::default();
    let (g, _) = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    d0.check(diagonal_ok(&g));
    let d = c
        .split(Some(0), -(r as i32))
        .map_err(e2s)?
        .ok_or("split returned no communicator")?;
    let a2 = sys.matrix(d.rank() as usize);
    let before = instance_counts();
    let res = solve(&d, &mut s, &sys, &a2);
    if let Ok((g, _)) = &res {
        // record the base defect's symptom (wrong values) for the report
        d0.check(diagonal_ok(g).map_err(|e| format!("reuse on the reversed communicator: {e}")));
    }
    ensure!(
        matches!(&res, Err(e) if e.to_string().contains("bound to another communicator")),
        "reuse on a reordered communicator was not refused: {:?}",
        res.as_ref().map(|_| "Ok")
    );
    no_native_job(before, "reordered communicator")?;
    // still usable on its own communicator (factors re-used)
    let (g, _) = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    ensure!(
        s.report().jobs == [3],
        "after the refusal: {:?}",
        s.report().jobs
    );
    d0.check(diagonal_ok(&g));
    // explicit rebinding: destroy on the bound communicator, then solve on
    // the reordered one
    s.destroy().map_err(e2s)?;
    let (g, _) = solve(&d, &mut s, &sys, &a2).map_err(e2s)?;
    ensure!(
        s.report().jobs == [-1, 1, 2, 3],
        "rebinding {:?}",
        s.report().jobs
    );
    d0.check(diagonal_ok(&g).map_err(|e| format!("after rebinding: {e}")));
    s.destroy().map_err(e2s)?;
    drop(d);
    d0.finish()
}

/// An instance bound to `c` is reused on a **subgroup** of `c`
/// (`split` into two halves): agreed refusal on every rank of both halves
/// (no rank calls the old group's collectives), then the explicit protocol
/// (destroy on `c`, fresh initialisation on each half).
fn mu01_subgroup(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let sys = upstream_diagonal(p);
    let a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    let mut d0 = Deferred::default();
    let (g, _) = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    d0.check(diagonal_ok(&g));
    let half = c
        .split(Some((r % 2) as u32), r as i32)
        .map_err(e2s)?
        .ok_or("split returned no communicator")?;
    let sys_h = upstream_diagonal(half.size());
    let a_h = sys_h.matrix(half.rank() as usize);
    let before = instance_counts();
    let res = solve(&half, &mut s, &sys_h, &a_h);
    // every rank of the world reaches this barrier: nobody is left inside
    // the old group's collectives
    c.barrier().map_err(e2s)?;
    ensure!(
        matches!(&res, Err(e) if e.to_string().contains("bound to another communicator")),
        "reuse on a subgroup was not refused: {:?}",
        res.as_ref().map(|_| "Ok")
    );
    no_native_job(before, "subgroup")?;
    s.destroy().map_err(e2s)?;
    let (g, _) = solve(&half, &mut s, &sys_h, &a_h).map_err(e2s)?;
    ensure!(
        s.report().jobs == [-1, 1, 2, 3] && s.report().nb_proc == half.size(),
        "subgroup after destroy {:?} on {} processes",
        s.report().jobs,
        s.report().nb_proc
    );
    d0.check(diagonal_ok(&g).map_err(|e| format!("subgroup after destroy: {e}")));
    s.destroy().map_err(e2s)?;
    drop(half);
    d0.finish()
}

// ------------------------------------------------------------------

/// Common shape of the divergent-option regressions: `setup(rank)` gives
/// each rank's options (and may run rank-local preparation); the first
/// solve on `c` must be refused on every rank before any native job, and
/// after `harmonise` the solver must solve correctly.
fn divergent(
    c: &Communicator,
    what: &str,
    retry_system: bool,
    setup: impl FnOnce(usize, &mut MumpsSolver) -> Outcome,
    harmonise: impl FnOnce(&mut MumpsSolver),
) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    if p < 2 {
        println!("INFO {what}: not applicable on one rank");
        return Ok(());
    }
    let sys = if retry_system {
        elasticity(6, p)
    } else {
        laplacian(5, true, p)
    };
    let a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    // rank-local preparation: its verdict is agreed before any collective
    let prepared = setup(r, &mut s);
    let mut ok = [u8::from(prepared.is_ok())];
    c.all_reduce(&mut ok, ReduceOp::Min).map_err(e2s)?;
    prepared?;
    ensure!(ok[0] == 1, "{what}: the preparation failed on another rank");
    let before = instance_counts();
    let t0 = Instant::now();
    let res = solve(c, &mut s, &sys, &a);
    println!(
        "INFO {what} rank {r}: {:?} after {:?}",
        res.as_ref().map(|_| "solved"),
        t0.elapsed()
    );
    ensure!(
        matches!(&res, Err(e) if e.to_string().contains("disagree on the MUMPS options")),
        "{what}: divergent options were not refused: {:?}",
        res.as_ref().map(|_| "Ok")
    );
    no_native_job(before, what)?;
    harmonise(&mut s);
    let (g, _) = solve(c, &mut s, &sys, &a).map_err(|e| format!("{what} harmonised: {e}"))?;
    // last statement of the scenario: a rank-0-only verdict is safe here
    residual_ok(&sys, &g, &format!("{what} after harmonising"))
}

/// `mu02_*` (case `ceiling`): `ICNTL(14) = −80` on every rank,
/// retry ceiling −50 on rank 0 and 90 elsewhere. The base runs into a
/// collective mismatch (rank 0 stops retrying while its peers retry).
fn mu02_divergent_ceiling(c: &Communicator) -> Outcome {
    divergent(
        c,
        "mu02_divergent_ceiling",
        true,
        |r, s| {
            s.options_mut().icntl.insert(14, -80);
            s.options_mut().icntl14_ceiling = Some(if r == 0 { -50 } else { 90 });
            Ok(())
        },
        |s| s.options_mut().icntl14_ceiling = Some(90),
    )
}

/// `mu02_*` (case `retry-rhs`): UD-216 compatibility on rank 0
/// only (extra solves after factorisation retries on one rank).
fn mu02_divergent_retry_rhs(c: &Communicator) -> Outcome {
    divergent(
        c,
        "mu02_divergent_retry_rhs",
        true,
        |r, s| {
            s.options_mut().icntl.insert(14, -80);
            if r == 0 {
                s.options_mut().retry_rhs = RetryRhs::UpstreamSolution;
            }
            Ok(())
        },
        |s| s.options_mut().retry_rhs = RetryRhs::Original,
    )
}

/// An `ICNTL` override (ordering, `ICNTL(7) = 5`) on the last rank
/// only.
fn mu02_divergent_icntl(c: &Communicator) -> Outcome {
    let last = c.size().saturating_sub(1);
    divergent(
        c,
        "mu02_divergent_icntl",
        false,
        move |r, s| {
            if r == last {
                s.options_mut().icntl.insert(7, 5);
            }
            Ok(())
        },
        |s| {
            s.options_mut().icntl.remove(&7);
        },
    )
}

/// A `CNTL` override (relative pivoting threshold `CNTL(1)`) on
/// rank 0 only.
fn mu02_divergent_cntl(c: &Communicator) -> Outcome {
    divergent(
        c,
        "mu02_divergent_cntl",
        false,
        |r, s| {
            if r == 0 {
                s.options_mut().cntl.insert(1, 0.1);
            }
            Ok(())
        },
        |s| {
            s.options_mut().cntl.remove(&1);
        },
    )
}

/// The **persisted** `ICNTL(14)` (kept after a workspace retry)
/// differs: rank 0 alone solved the retry system on its own communicator
/// (retries −80 → 0), destroyed that instance, and now joins the others
/// with the same options but a different effective `ICNTL(14)`.
fn mu02_divergent_persisted_icntl14(c: &Communicator) -> Outcome {
    let me = c
        .split(Some(c.rank().cast_unsigned()), 0)
        .map_err(e2s)?
        .ok_or("self split")?;
    let out = divergent(
        c,
        "mu02_divergent_persisted_icntl14",
        true,
        |r, s| {
            s.options_mut().icntl.insert(14, -80);
            if r == 0 {
                let sys1 = elasticity(6, 1);
                let a1 = sys1.matrix(0);
                solve(&me, s, &sys1, &a1).map_err(|e| format!("rank 0 alone: {e}"))?;
                ensure!(
                    s.report().factorization_retries > 0,
                    "rank 0 alone: no retry was forced"
                );
                s.destroy().map_err(e2s)?;
            }
            Ok(())
        },
        |s| {
            // the agreed way out: a fresh solver state on every rank
            *s = MumpsSolver::with_options(s.options().clone());
        },
    );
    drop(me);
    out
}

/// A one-rank mutation between solves (rank 0 changes the ordering
/// and invalidates its factors; then rank 0 changes only the ceiling) is
/// refused on every rank before any native job; reverting restores the
/// solver.
fn mu02_one_rank_mutation(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    if p < 2 {
        println!("INFO mu02_one_rank_mutation: not applicable on one rank");
        return Ok(());
    }
    let sys = laplacian(5, true, p);
    let a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    let mut d0 = Deferred::default();
    solve(c, &mut s, &sys, &a).map_err(e2s)?;
    for case in ["icntl+invalidate", "ceiling only"] {
        if r == 0 {
            if case == "icntl+invalidate" {
                s.options_mut().icntl.insert(7, 5);
                s.invalidate();
            } else {
                s.options_mut().icntl14_ceiling = Some(50);
            }
        }
        let before = instance_counts();
        let res = solve(c, &mut s, &sys, &a);
        ensure!(
            matches!(&res, Err(e) if e.to_string().contains("disagree on the MUMPS options")),
            "{case}: one-rank mutation not refused: {:?}",
            res.as_ref().map(|_| "Ok")
        );
        no_native_job(before, case)?;
        if r == 0 {
            s.options_mut().icntl.remove(&7);
            s.options_mut().icntl14_ceiling = None;
        }
        let (g, _) = solve(c, &mut s, &sys, &a).map_err(|e| format!("{case} reverted: {e}"))?;
        d0.check(residual_ok(&sys, &g, &format!("{case} after reverting")));
    }
    d0.finish()
}

// ------------------------------------------------------- /

/// The operation a worker thread of rank 0 has in flight on a helper
/// communicator while the main thread runs MUMPS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerOp {
    Duplicate,
    Split,
    Native,
}

/// The MUMPS operation of the main thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainOp {
    FirstInit,
    Reinit,
    Destroy,
}

fn worker_op(h: &Communicator, op: WorkerOp) -> Result<()> {
    match op {
        WorkerOp::Duplicate => h.duplicate().map(drop),
        WorkerOp::Split => h.split(Some(0), 0).map(drop),
        WorkerOp::Native => h.native_collective("v54:worker-native", Ok(()), |_| ()),
    }
}

/// Outcome class of an operation under contention (agreed check below).
fn class(r: &Result<()>) -> u64 {
    match r {
        Ok(()) => 0,
        Err(Error::Communication(m))
            if m.contains("refused") || m.contains("validation failed") =>
        {
            1
        }
        Err(_) => 2,
    }
}

/// Run `main` on rank 0's main thread while rank 0's worker has `op` in
/// flight on a helper communicator (started 300 ms before; the other ranks
/// enter `op` only after `main`, so the worker's operation is pending
/// throughout). Returns `main`'s result and the worker's.
fn under_contention<T>(
    c: &Communicator,
    op: WorkerOp,
    main: impl FnOnce() -> Result<T>,
) -> std::result::Result<(Result<T>, Result<()>), String> {
    let helper = c.duplicate().map_err(e2s)?;
    let worker = if c.rank() == 0 {
        let h = helper.clone();
        Some(std::thread::spawn(move || worker_op(&h, op)))
    } else {
        None
    };
    std::thread::sleep(Duration::from_millis(300));
    let out = main();
    let w = match worker {
        Some(w) => w
            .join()
            .unwrap_or_else(|_| Err(Error::invalid("worker panicked"))),
        None => worker_op(&helper, op),
    };
    drop(helper);
    Ok((out, w))
}

/// `mu04_*` (case `refused-reinit`): after a successful solve, a
/// symmetry promotion forces a re-initialisation while rank 0's worker
/// duplicates a helper communicator: the native destruction of the old
/// instance is refused on every rank. The old instance must stay owned by
/// the solver; after the contention clears the retry destroys it
/// (`JOB = -2`) before re-initialising, and every native initialisation
/// has exactly one native destruction at the end.
fn mu04_refused_reinit(c: &Communicator) -> Outcome {
    if level() != ThreadLevel::Serialized {
        println!("INFO mu04_refused_reinit: needs MPI_THREAD_SERIALIZED, not applicable");
        return Ok(());
    }
    if c.size() < 2 {
        // the worker's duplication completes at once without a peer that
        // delays it: no contention can be established
        println!("INFO mu04_refused_reinit: needs two ranks, not applicable");
        return Ok(());
    }
    let (r, p) = (c.rank() as usize, c.size());
    let start = instance_counts();
    let sys = laplacian(5, true, p);
    let mut a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    solve(c, &mut s, &sys, &a).map_err(e2s)?;
    a.promote_to_unsymmetric();
    let mut sys_u = sys.clone();
    sys_u.sym = false;
    let (res, w) = under_contention(c, WorkerOp::Duplicate, || {
        solve(c, &mut s, &sys_u, &a).map(drop)
    })?;
    let mut v = [class(&res), u64::from(w.is_ok())];
    let all = c.all_gather(&v).map_err(e2s)?;
    ensure!(
        all.chunks(2).all(|x| x == all.get(..2).unwrap_or(&[])),
        "outcomes differ between ranks: {all:?}"
    );
    v.copy_from_slice(all.get(..2).unwrap_or(&[9, 9]));
    ensure!(
        v[0] == 1,
        "contention did not refuse the re-initialisation (class {}, {:?}); the scenario needs \
         the refusal",
        v[0],
        res.as_ref().err()
    );
    ensure!(v[1] == 1, "the worker's duplication failed: {w:?}");
    let (g, _) = solve(c, &mut s, &sys_u, &a).map_err(e2s)?;
    ensure!(
        s.report().jobs == [-2, -1, 1, 2, 3],
        "retried re-initialisation jobs {:?} (the old instance must be destroyed first)",
        s.report().jobs
    );
    // The promotion keeps the global operator: check against `sys`
    // (`sys_u` only switches the storage type of the solve). Rank-0 data:
    // deferred, every rank reaches the barrier below.
    let mut d0 = Deferred::default();
    d0.check(residual_ok(&sys, &g, "after the retried re-initialisation"));
    drop(s); // queued JOB = -2: runs at once on MPI's main thread
    c.barrier().map_err(e2s)?;
    balanced(start, "mu04_refused_reinit")?;
    d0.finish()
}

/// The gathered solution (rank 0) solves `sys` to the residual smoke
/// threshold `1e-10` (NUMERICAL_VALIDATION).
fn residual_ok(sys: &System, g: &Option<Vec<Real>>, what: &str) -> std::result::Result<(), String> {
    match g {
        Some(x) => {
            let res = sys.residual(x);
            if res < 1e-10 {
                Ok(())
            } else {
                Err(format!("{what}: residual {res:e}"))
            }
        }
        None => Ok(()),
    }
}

/// Controlled interleavings of a pending duplicate / split / native
/// collective on rank 0's worker (helper communicator) with a first
/// initialisation, a re-initialisation and an explicit destruction on the
/// main thread, at every rank count. Each outcome must be identical on
/// every rank (success or an agreed refusal), a refused operation must
/// succeed once the contention cleared, the solver must stay usable, and
/// no native instance may be orphaned.
fn mu05_admission_interleavings(c: &Communicator) -> Outcome {
    if level() != ThreadLevel::Serialized {
        println!("INFO mu05_admission_interleavings: needs MPI_THREAD_SERIALIZED, not applicable");
        return Ok(());
    }
    let (r, p) = (c.rank() as usize, c.size());
    let start = instance_counts();
    let sys = laplacian(4, true, p);
    let mut sys_u = sys.clone();
    sys_u.sym = false;
    let mut d0 = Deferred::default();
    for main_op in [MainOp::FirstInit, MainOp::Reinit, MainOp::Destroy] {
        for op in [WorkerOp::Duplicate, WorkerOp::Split, WorkerOp::Native] {
            let tag = format!("{main_op:?} x {op:?}");
            let mut a = sys.matrix(r);
            let mut s = MumpsSolver::default();
            if main_op != MainOp::FirstInit {
                solve(c, &mut s, &sys, &a).map_err(|e| format!("{tag} setup: {e}"))?;
            }
            if main_op == MainOp::Reinit {
                a.promote_to_unsymmetric();
            }
            let target = if main_op == MainOp::Reinit {
                &sys_u
            } else {
                &sys
            };
            let run = |s: &mut MumpsSolver| -> Result<()> {
                match main_op {
                    MainOp::Destroy => s.destroy(),
                    _ => solve(c, s, target, &a).map(drop),
                }
            };
            let (res, w) = under_contention(c, op, || run(&mut s))?;
            let v = [class(&res), class(&w)];
            let all = c.all_gather(&v).map_err(e2s)?;
            let first = [all[0], all[1]];
            println!(
                "INFO mu05 {tag} rank {r}: main {:?}, worker {:?}",
                res.as_ref().map(|_| "ok"),
                w.as_ref().map(|_| "ok")
            );
            ensure!(
                all.chunks(2).all(|x| x == first),
                "{tag}: outcomes differ between ranks {all:?}"
            );
            ensure!(
                first[0] <= 1 && first[1] <= 1,
                "{tag}: neither success nor an agreed refusal ({:?} / {:?})",
                res.as_ref().err(),
                w.as_ref().err()
            );
            if first[0] == 1 {
                run(&mut s).map_err(|e| format!("{tag}: retry after the contention: {e}"))?;
            }
            // continued usability (the promotion keeps the global operator:
            // residual of `sys`)
            let (g, _) = solve(c, &mut s, target, &a).map_err(|e| format!("{tag} then: {e}"))?;
            d0.check(residual_ok(&sys, &g, &tag));
            s.destroy()
                .map_err(|e| format!("{tag}: final destroy: {e}"))?;
        }
    }
    c.barrier().map_err(e2s)?;
    balanced(start, "mu05_admission_interleavings")?;
    d0.finish()
}

type Scenario = (&'static str, fn(&Communicator) -> Outcome);

const SCENARIOS: [Scenario; 10] = [
    ("mu01_reordered_communicator", mu01_reordered_communicator),
    ("mu01_subgroup", mu01_subgroup),
    ("mu02_divergent_ceiling", mu02_divergent_ceiling),
    ("mu02_divergent_retry_rhs", mu02_divergent_retry_rhs),
    ("mu02_divergent_icntl", mu02_divergent_icntl),
    ("mu02_divergent_cntl", mu02_divergent_cntl),
    (
        "mu02_divergent_persisted_icntl14",
        mu02_divergent_persisted_icntl14,
    ),
    ("mu02_one_rank_mutation", mu02_one_rank_mutation),
    ("mu04_refused_reinit", mu04_refused_reinit),
    ("mu05_admission_interleavings", mu05_admission_interleavings),
];

fn main() {
    let watchdog = std::env::var("AKANTU_MUMPS_WATCHDOG_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300u64);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(watchdog));
        eprintln!("review_v54_regressions: watchdog ({watchdog} s) expired");
        std::process::exit(70);
    });
    let only = std::env::args().nth(1);
    if let Some(o) = &only
        && !SCENARIOS.iter().any(|(n, _)| n == o)
    {
        eprintln!("unknown scenario {o}");
        std::process::exit(2);
    }
    let control = world();
    let work = world();
    let rank = control.rank();
    if rank == 0 {
        println!(
            "INFO review_v54_regressions: ranks={} thread_level={:?}",
            control.size(),
            level()
        );
    }
    let mut failed = 0;
    for (name, f) in SCENARIOS {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let t0 = Instant::now();
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&work)))
            .unwrap_or_else(|_| Err("panicked".into()));
        let mut ok = [u8::from(out.is_ok())];
        control
            .all_reduce(&mut ok, ReduceOp::Min)
            .expect("control reduction");
        if ok[0] == 0 {
            failed += 1;
        }
        if let Err(e) = &out {
            println!("rank {rank}: {name}: {e}");
        }
        if rank == 0 {
            println!(
                "{} {name} ({:.2} s)",
                if ok[0] == 1 { "PASS" } else { "FAIL" },
                t0.elapsed().as_secs_f64()
            );
        }
    }
    if rank == 0 {
        println!("review_v54_regressions: failed={failed}");
    }
    drop(work);
    drop(control);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
