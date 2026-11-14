// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression scenarios of
// the MUMPS backend ported from Akantu (EPFL/LSMS)
// src/solver/sparse_solver_mumps.cc (initialize, analysis/factorize/solve,
// mumpsDataDestroy): stale coordinates after a refused solve, the
// re-use key of a different matrix, teardown on a broken
// communicator and concurrent native jobs under
// `MPI_THREAD_MULTIPLE`.

//! Real-MPI regressions (`harness = false`), one process per
//! scenario or every scenario in order:
//!
//! ```text
//! mpirun -n N review_v66_regressions [scenario]
//! AKANTU_MUMPS_THREAD_LEVEL=multiple mpirun -n N review_v66_regressions mu2_05_concurrent_solvers
//! ```
//!
//! Scenarios run their collectives to completion on every rank (rank-0
//! data is checked at the end); the verdict is agreed on a control
//! communicator; a watchdog bounds the process (a scenario that hangs or
//! crashes on the defective base fails by the watchdog, the signal or the
//! runner's `timeout`). Scenarios that need several ranks
//! (`mu2_01_*`) or a thread level (`mu2_05_*`: `multiple`; `pe01_*`:
//! `serialized` and >= 2 ranks) print "not applicable" and pass elsewhere.
//! `pe01_*` (the pattern of the PETSc test of that name, checked for
//! MUMPS): an agreed admission refusal of a native collective must stay an
//! ordinary error.

mod mumps_common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, MpiOptions, ReduceOp};
use akantu_core::{Error, FatalKind, GlobalIdx, Real, Result};
use akantu_linalg::SparseMatrixAij;
use akantu_linalg::centralize::gather_owned_to_root;
use akantu_linalg::direct::SparseDirectSolver;
use akantu_linalg::mumps::{MumpsSolver, instance_counts};

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

fn e2s(e: Error) -> String {
    e.to_string()
}

/// Solve `sys` (rank-partial matrix `a`) with the consistent layout;
/// returns the gathered solution on rank 0.
fn solve(
    c: &Communicator,
    s: &mut MumpsSolver,
    sys: &System,
    a: &SparseMatrixAij,
) -> Result<Option<Vec<Real>>> {
    let (r, p) = (c.rank() as usize, c.size());
    let l = layout(sys.n, p, r);
    let rhs = local_rhs(&l, &sys.b);
    let mut x = vec![0.0; l.nb_local()];
    let mut ph = Phases::new(c, "v66");
    s.solve_distributed(&mut ph, "t", &l, Ok(a), &rhs, &mut x)?;
    gather_owned_to_root(&mut ph, "g", &l, &x)
}

/// A solve of `b` with an **inconsistent** layout (every rank owns every
/// equation): the preflight passes, the plan asks for an analysis (another
/// matrix), the coordinates step converts `b`'s coordinates, then the
/// right-hand-side gather refuses ("the ranks own …"), which left stale
/// coordinates before the fix. Returns the (agreed) result.
fn refused_solve(
    c: &Communicator,
    s: &mut MumpsSolver,
    sys: &System,
    b: &SparseMatrixAij,
) -> Result<()> {
    let bad = layout(sys.n, 1, 0);
    let rhs = local_rhs(&bad, &sys.b);
    let mut x = vec![0.0; bad.nb_local()];
    let mut ph = Phases::new(c, "v66-bad");
    s.solve_distributed(&mut ph, "t", &bad, Ok(b), &rhs, &mut x)
}

/// Relative residual of a gathered solution (rank 0), `None` elsewhere.
fn residual_of(sys: &System, g: &Option<Vec<Real>>) -> Option<Real> {
    g.as_ref().map(|x| sys.residual(x))
}

// ------------------------------------------------------------------

/// Solve `A`, then a solve of another matrix `B` that is refused
/// **after** its coordinates were converted (RHS gather failure), then `A`
/// again with new values. The refused solve must leave the solver's
/// analysis and coordinates consistent: the third solve refactorises `A`
/// (`[2, 3]`) with `A`'s coordinates and solves it (residual < 1e-10).
/// `shorter`: `B` has fewer local entries than `A` (before the fix MUMPS
/// read `IRN_loc`/`JCN_loc` past the end of the Rust allocation, SIGSEGV
/// at 2 and 4 ranks); otherwise more (the base factorised `A`'s values at
/// `B`'s first coordinates: silently wrong).
fn stale_coordinates(c: &Communicator, shorter: bool) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    if p < 2 {
        println!("INFO not applicable on one rank (the inconsistent layout needs >= 2 ranks)");
        return Ok(());
    }
    let sys = laplacian(5, false, p);
    let mut a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    let g = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    ensure!(
        s.report().jobs == [-1, 1, 2, 3],
        "first solve: {:?}",
        s.report().jobs
    );
    let res_first = residual_of(&sys, &g);
    let mut sys_b = sys.clone();
    if shorter {
        // fewer local entries: the diagonal only
        for part in &mut sys_b.per_rank {
            part.retain(|e| e.0 == e.1);
        }
    } else {
        // more local entries: couplings to equations two apart added
        // **before** A's entries, so that B's first `nnz(A)` coordinates are
        // not A's (a stale prefix of B's coordinates is then wrong)
        let n = sys.n as GlobalIdx;
        for part in &mut sys_b.per_rank {
            let extra: Vec<_> = part
                .iter()
                .filter(|e| e.0 == e.1)
                .map(|e| (e.0, (e.0 + 2) % n, 1e-3))
                .collect();
            part.splice(0..0, extra);
        }
    }
    let b = sys_b.matrix(r);
    println!(
        "INFO rank {r}: local nnz A = {}, B = {} ({})",
        a.nnz(),
        b.nnz(),
        if shorter { "shorter" } else { "longer" }
    );
    let refused = refused_solve(c, &mut s, &sys_b, &b);
    ensure!(
        matches!(&refused, Err(e) if e.to_string().contains("own")),
        "the solve with the inconsistent layout was not refused by the RHS gather: {:?}",
        refused.map_err(e2s)
    );
    // new values for A only (value release +1, profile unchanged)
    let (i, j, _) = sys.per_rank[r][0];
    a.add(i, j, 0.0).map_err(e2s)?;
    let g = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    let jobs = s.report().jobs.clone();
    let res = residual_of(&sys, &g);
    if let (Some(res), Some(first)) = (res, res_first) {
        println!(
            "INFO rank 0: A first residual {first:.3e}; A after the refused solve: jobs {jobs:?}, \
             residual {res:.3e}"
        );
        ensure!(
            res < 1e-10,
            "A after the refused solve: relative residual {res:e}"
        );
    }
    ensure!(
        jobs == [2, 3],
        "A after the refused solve: jobs {jobs:?} (expected a refactorisation of the analysis of A)"
    );
    Ok(())
}

fn mu2_01_stale_coordinates_shorter(c: &Communicator) -> Outcome {
    stale_coordinates(c, true)
}

fn mu2_01_stale_coordinates_longer(c: &Communicator) -> Outcome {
    stale_coordinates(c, false)
}

// ------------------------------------------------------------------

/// Two different matrices `A` and `B` (transposed add order) with
/// identical order, type, entry counts and release counters, solved with
/// the **same** solver: `B` must be analysed and factorised (`[1, 2, 3]`)
/// and solved (residual < 1e-10). Before the fix it re-used `A`'s factors
/// (`[3]`, relative residual 3.77). Also a third matrix that takes `B`'s
/// profile through `copy_profile` with an equal profile release.
fn mu2_02_release_collision(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let sys = laplacian(5, false, p);
    let a = sys.matrix(r);
    let mut sys_b = sys.clone();
    for part in &mut sys_b.per_rank {
        for e in part.iter_mut() {
            std::mem::swap(&mut e.0, &mut e.1);
        }
    }
    let b = sys_b.matrix(r);
    println!(
        "INFO rank {r}: releases A ({}, {}), B ({}, {}), nnz {} / {}",
        a.profile_release(),
        a.value_release(),
        b.profile_release(),
        b.value_release(),
        a.nnz(),
        b.nnz()
    );
    let mut s = MumpsSolver::default();
    let ga = solve(c, &mut s, &sys, &a).map_err(e2s)?;
    let gb = solve(c, &mut s, &sys_b, &b).map_err(e2s)?;
    let jobs_b = s.report().jobs.clone();
    // copy_profile: `m` (built like A, so its releases equal A's) takes B's
    // profile and release, then B's values; the solver last saw B.
    let mut m = sys.matrix(r);
    m.copy_profile(&b);
    m.copy_content(&b).map_err(e2s)?;
    let mut s2 = MumpsSolver::default();
    let gm_a = solve(c, &mut s2, &sys, &a).map_err(e2s)?;
    let mut m2 = sys.matrix(r);
    m2.copy_profile(&a);
    m2.copy_content(&a).map_err(e2s)?;
    // `m2` has A's profile release but is another matrix object: s2 must
    // re-analyse (its coordinates are A's, so the answer would be right
    // either way — the jobs show the identity is checked)
    let gm2 = solve(c, &mut s2, &sys, &m2).map_err(e2s)?;
    let jobs_m2 = s2.report().jobs.clone();
    let gm = solve(c, &mut s2, &sys_b, &m).map_err(e2s)?;
    let jobs_m = s2.report().jobs.clone();
    if let (Some(ra), Some(rb), Some(rm), Some(rm2), Some(rma)) = (
        residual_of(&sys, &ga),
        residual_of(&sys_b, &gb),
        residual_of(&sys_b, &gm),
        residual_of(&sys, &gm2),
        residual_of(&sys, &gm_a),
    ) {
        println!(
            "INFO rank 0: A residual {ra:.3e}; B with the same solver: jobs {jobs_b:?}, residual \
             {rb:.3e}; copy_profile(A) {rm2:.3e} jobs {jobs_m2:?}; copy_profile(B) {rm:.3e} jobs \
             {jobs_m:?} (A on that solver {rma:.3e})"
        );
        ensure!(
            rb < 1e-10,
            "B solved with A's factors: relative residual {rb:e}"
        );
        ensure!(
            rm < 1e-10,
            "copy_profile(B) solved with stale factors: residual {rm:e}"
        );
        ensure!(rm2 < 1e-10, "copy_profile(A): residual {rm2:e}");
    }
    ensure!(
        jobs_b == [1, 2, 3],
        "B with the same solver: jobs {jobs_b:?} (expected a new analysis: another matrix)"
    );
    ensure!(
        jobs_m2 == [1, 2, 3] && jobs_m == [1, 2, 3],
        "copy_profile targets: jobs {jobs_m2:?} / {jobs_m:?} (expected new analyses)"
    );
    Ok(())
}

// ------------------------------------------------------------------

/// Dropping (or destroying) a solver whose bound communicator is
/// **broken** must not enter the blocking collective `JOB = -2`: the
/// instance is abandoned (leaked), `destroy()` returns
/// `Error::Fatal { kind: Transport }`, nothing hangs. Rank 0 tears down
/// first while its peers wait on another communicator, so a `JOB = -2` on
/// rank 0 would wait for ever (before the fix; one rank: the count check
/// fails instead).
fn mu2_04_teardown_on_broken_communicator(c: &Communicator) -> Outcome {
    let (r, p) = (c.rank() as usize, c.size());
    let w = world(); // its own family: breaking it leaves `c` usable
    let sys = upstream_diagonal(p);
    let a = sys.matrix(r);
    let mut dropped = MumpsSolver::default();
    let mut destroyed = MumpsSolver::default();
    solve(&w, &mut dropped, &sys, &a).map_err(e2s)?;
    solve(&w, &mut destroyed, &sys, &a).map_err(e2s)?;
    let before = instance_counts();
    // every rank observes the transport failure (marks the family broken)
    Phases::new(&w, "v66-break").mark_broken();
    ensure!(w.broken().is_some(), "the communicator is not broken");
    let mut destroy_result = Ok(());
    let teardown = |dropped: MumpsSolver, destroyed: &mut MumpsSolver| {
        let t0 = Instant::now();
        drop(dropped);
        let res = destroyed.destroy();
        (res, t0.elapsed())
    };
    let mut elapsed = Duration::ZERO;
    let mut dropped = Some(std::mem::take(&mut dropped));
    if r == 0 {
        let (res, t) = teardown(dropped.take().unwrap_or_default(), &mut destroyed);
        destroy_result = res;
        elapsed = t;
    }
    c.barrier().map_err(e2s)?;
    if r != 0 {
        let (res, t) = teardown(dropped.take().unwrap_or_default(), &mut destroyed);
        destroy_result = res;
        elapsed = t;
    }
    c.barrier().map_err(e2s)?;
    let now = instance_counts();
    println!(
        "INFO rank {r}: teardown on the broken communicator took {:.3} s; destroy(): {:?}; \
         destructions {} abandoned {}",
        elapsed.as_secs_f64(),
        destroy_result.as_ref().map_err(|e| e.to_string()),
        now.destroyed - before.destroyed,
        now.abandoned - before.abandoned
    );
    ensure!(
        now.destroyed == before.destroyed,
        "{} JOB = -2 ran on a broken communicator",
        now.destroyed - before.destroyed
    );
    ensure!(
        now.abandoned - before.abandoned == 2,
        "{} instances abandoned (expected 2: the dropped and the destroyed solver)",
        now.abandoned - before.abandoned
    );
    ensure!(
        matches!(&destroy_result, Err(e) if e.fatal_kind() == Some(FatalKind::Transport)),
        "destroy() on a broken communicator: {:?} (expected a transport-fatal error)",
        destroy_result.map_err(e2s)
    );
    ensure!(
        destroyed.bound_communicator().is_none(),
        "the destroyed solver still holds its instance"
    );
    Ok(())
}

// ------------------------------------------------------------------

/// Under `MPI_THREAD_MULTIPLE`, two MUMPS solvers driven by two
/// threads (each on its own duplicate communicator) must never run native
/// jobs at the same time in a process: concurrent native jobs are refused
/// (agreed, ordinary error) and retried here with back-off. Every solve
/// must end correct; the process high-water mark of concurrent native jobs
/// must stay 1 (before the fix they ran concurrently).
///
/// The final `destroy()` (`JOB = -2`) is a native job too: when one thread
/// finishes first, its destruction can meet the other thread's analysis or
/// factorisation and is refused like a solve. `MumpsSolver::destroy`
/// documents that a refusal keeps the instance and that the call can be
/// repeated, so it is retried under the same rule as the solves (the
/// test used to give up at the first refused destruction, ≈ 7 % of the
/// launches on x86_64). Each instance must then be destroyed exactly once
/// (`JOB = -2` count) and none abandoned.
fn mu2_05_concurrent_solvers(c: &Communicator) -> Outcome {
    if level() != ThreadLevel::Multiple {
        println!(
            "INFO not applicable at {:?} (needs MPI_THREAD_MULTIPLE)",
            level()
        );
        return Ok(());
    }
    let (r, p) = (c.rank() as usize, c.size());
    let d1 = c.duplicate().map_err(e2s)?;
    let d2 = c.duplicate().map_err(e2s)?;
    let sys = laplacian(12, true, p);
    let a = sys.matrix(r);
    const SOLVES: usize = 6;
    const ATTEMPTS: usize = 400;
    let before = instance_counts();
    let drive = |d: Communicator,
                 pause_ms: u64|
     -> std::result::Result<(usize, usize, Vec<Real>), String> {
        let mut s = MumpsSolver::default();
        let mut refusals = 0;
        let mut worst: Vec<Real> = Vec::new();
        for _ in 0..SOLVES {
            s.invalidate(); // force analysis + factorisation every time
            let mut attempt = 0;
            loop {
                match solve(&d, &mut s, &sys, &a) {
                    Ok(g) => {
                        if let Some(res) = residual_of(&sys, &g) {
                            worst.push(res);
                        }
                        break;
                    }
                    // A refusal is an ordinary error agreed on every rank (its
                    // text differs: the refused rank's own reason, "local
                    // validation failed on rank …" elsewhere), so the retry
                    // decision is identical on every rank.
                    Err(e) if e.fatal_kind().is_none() && attempt < ATTEMPTS => {
                        if refusals == 0 {
                            println!("INFO rank {r}: first refusal: {e}");
                        }
                        refusals += 1;
                        attempt += 1;
                        std::thread::sleep(Duration::from_millis(
                            pause_ms * (1 + attempt as u64 % 5),
                        ));
                    }
                    Err(e) => return Err(format!("solve failed after {attempt} refusals: {e}")),
                }
            }
        }
        // Same rule for the destruction (see the doc comment): an ordinary
        // error is the agreed refusal, the instance is kept, retry.
        let mut destroy_refusals = 0;
        loop {
            match s.destroy() {
                Ok(()) => break,
                Err(e) if e.fatal_kind().is_none() && destroy_refusals < ATTEMPTS => {
                    if destroy_refusals == 0 {
                        println!("INFO rank {r}: first destroy refusal: {e}");
                    }
                    ensure!(
                        s.bound_communicator().is_some(),
                        "a refused destroy() dropped the instance: {e}"
                    );
                    destroy_refusals += 1;
                    std::thread::sleep(Duration::from_millis(
                        pause_ms * (1 + destroy_refusals as u64 % 5),
                    ));
                }
                Err(e) => {
                    return Err(format!(
                        "destroy failed after {destroy_refusals} refusals: {e}"
                    ));
                }
            }
        }
        Ok((refusals, destroy_refusals, worst))
    };
    let (o1, o2) = std::thread::scope(|sc| {
        let h1 = sc.spawn(|| drive(d1, 1));
        let h2 = sc.spawn(|| drive(d2, 3));
        (
            h1.join()
                .unwrap_or_else(|_| Err("thread 1 panicked".into())),
            h2.join()
                .unwrap_or_else(|_| Err("thread 2 panicked".into())),
        )
    });
    let counts = instance_counts();
    println!(
        "INFO rank {r}: thread 1 {:?} refusals, thread 2 {:?} refusals (solve, destroy); max \
         concurrent native jobs {}",
        o1.as_ref().map(|o| (o.0, o.1)),
        o2.as_ref().map(|o| (o.0, o.1)),
        counts.max_concurrent_jobs
    );
    let (o1, o2) = (o1?, o2?);
    for (k, res) in o1.2.iter().chain(&o2.2).enumerate() {
        ensure!(*res < 1e-10, "solve {k}: relative residual {res:e}");
    }
    ensure!(
        counts.max_concurrent_jobs <= 1,
        "{} native MUMPS jobs ran at the same time in one process",
        counts.max_concurrent_jobs
    );
    ensure!(
        counts.destroyed - before.destroyed == 2,
        "{} JOB = -2 calls for the two solvers (expected exactly 2: a refused destroy() must \
         not run it)",
        counts.destroyed - before.destroyed
    );
    ensure!(
        counts.abandoned == before.abandoned,
        "{} instances abandoned",
        counts.abandoned - before.abandoned
    );
    Ok(())
}

// ------------------------------------------------------------------

/// Outcome class (agreed below): 0 success, 1 ordinary (recoverable) error,
/// 2 fatal (any kind).
fn class<T>(r: &Result<T>) -> u64 {
    match r {
        Ok(_) => 0,
        Err(e) if e.fatal_kind().is_none() => 1,
        Err(_) => 2,
    }
}

/// Every rank reports the same classes `v`; returns them.
fn agreed_classes(c: &Communicator, v: &[u64]) -> std::result::Result<Vec<u64>, String> {
    let all = c.all_gather(v).map_err(e2s)?;
    let first = all[..v.len()].to_vec();
    ensure!(
        all.chunks(v.len()).all(|x| x == first.as_slice()),
        "outcome classes differ between ranks: {all:?}"
    );
    Ok(first)
}

/// Neither the solver's communicator nor the sibling is broken on this rank.
fn not_broken(c: &Communicator, sibling: &Communicator, what: &str) -> Outcome {
    ensure!(
        c.broken().is_none() && sibling.broken().is_none(),
        "{what}: communicator broken after an admission refusal: {:?} / {:?}",
        c.broken(),
        sibling.broken()
    );
    Ok(())
}

/// Forward order: under
/// `MPI_THREAD_SERIALIZED`, rank 0's worker has a **duplication** (or a
/// split) of a sibling communicator pending while the solver enters its
/// native job — the first initialisation (`JOB = -1`) and, on an
/// initialised solver, a refactorisation (`JOB = 2`). The admission refusal
/// must be an ordinary error on every rank (never `Fatal`), no communicator
/// broken, the sibling operation unaffected (it completes), and the solver
/// usable afterwards (the retried solve is correct).
fn pe01_refusal_forward(c: &Communicator) -> Outcome {
    if level() != ThreadLevel::Serialized || c.size() < 2 {
        println!(
            "INFO not applicable at {:?} / {} rank(s) (needs MPI_THREAD_SERIALIZED and >= 2 ranks)",
            level(),
            c.size()
        );
        return Ok(());
    }
    let (r, p) = (c.rank() as usize, c.size());
    let sys = laplacian(5, true, p);
    let mut checked = Vec::new();
    for refactor in [false, true] {
        for split in [false, true] {
            let tag = format!(
                "{} x pending {}",
                if refactor {
                    "refactorisation"
                } else {
                    "first initialisation"
                },
                if split { "split" } else { "duplication" }
            );
            let mut a = sys.matrix(r);
            let mut s = MumpsSolver::default();
            if refactor {
                solve(c, &mut s, &sys, &a).map_err(|e| format!("{tag}: setup: {e}"))?;
                let (i, j, _) = sys.per_rank[r][0];
                a.add(i, j, 0.0).map_err(e2s)?; // new values: [2, 3]
            }
            let sibling = c.duplicate().map_err(e2s)?;
            let worker = (r == 0).then(|| {
                let h = sibling.clone();
                std::thread::spawn(move || {
                    if split {
                        h.split(Some(0), 0).map(drop)
                    } else {
                        h.duplicate().map(drop)
                    }
                })
            });
            std::thread::sleep(Duration::from_millis(300));
            let main = solve(c, &mut s, &sys, &a);
            let w = match worker {
                Some(w) => w
                    .join()
                    .unwrap_or_else(|_| Err(Error::invalid("worker panicked"))),
                None if split => sibling.split(Some(0), 0).map(drop),
                None => sibling.duplicate().map(drop),
            };
            let cls = agreed_classes(c, &[class(&main), class(&w)])?;
            if r == 0 {
                println!(
                    "INFO pe01 forward {tag}: main {:?}, sibling {:?}",
                    main.as_ref().map(|_| "ok").map_err(|e| e.to_string()),
                    w.as_ref().map(|_| "ok").map_err(|e| e.to_string())
                );
            }
            ensure!(cls[0] < 2 && cls[1] < 2, "{tag}: fatal outcome {cls:?}");
            if !split {
                // a pending duplication always refuses the split-class job
                ensure!(
                    cls[0] == 1,
                    "{tag}: the native job was not refused ({cls:?})"
                );
            }
            ensure!(cls[1] == 0, "{tag}: the sibling operation failed ({cls:?})");
            not_broken(c, &sibling, &tag)?;
            let g = solve(c, &mut s, &sys, &a).map_err(|e| format!("{tag}: retry: {e}"))?;
            if let Some(res) = residual_of(&sys, &g) {
                ensure!(res < 1e-10, "{tag}: retry residual {res:e}");
            }
            checked.push(cls[0]);
            s.destroy().map_err(|e| format!("{tag}: destroy: {e}"))?;
        }
    }
    println!("INFO rank {r}: pe01 forward main classes {checked:?} (1 = agreed refusal)");
    Ok(())
}

/// Attempts of [`pe01_refusal_reverse`] to start the duplication while a
/// native job is in flight. Natively the
/// first attempt hits (40 / 40 launches at 2 and 4 ranks); under valgrind's
/// default scheduler one attempt misses 40–75 % of the time
/// (E2, E4) and 79 % in a later measurement (76 misses in 96 attempts, first hit at
/// attempt 1–13 in 20 launches), so twenty attempts would still all miss
/// about 0.9 % of the time; sixty make it about 10⁻⁶ (0.79⁶⁰) and cost at
/// most about 35 s under valgrind (≈ 0.55 s per missed attempt).
const PE01_REVERSE_ATTEMPTS: usize = 60;

/// Reverse order: the solver's native job is **in
/// flight** (registered, holding the `Serialized` lock) when rank 0's
/// worker starts a duplication of a sibling communicator. The duplication
/// must be refused as an ordinary error on every rank (never `Fatal`), the
/// solve unaffected, nothing broken, and the duplication must succeed when
/// retried.
///
/// Whether the worker's duplication starts inside the native job is a
/// harness condition, not a library result: rank
/// 0's worker spins until a `dmumps_c` call of the main thread is in flight
/// (no sleep, no yield: the main thread must be inside the job when the
/// worker acts) and gives up as soon as the main thread's solve has
/// returned. Rank 0's measured flag is agreed on every rank on its own
/// (`Min`, the other ranks contribute 1), before the outcome classes, and a
/// missed window repeats the attempt (`invalidate`, solve, duplication) up
/// to [`PE01_REVERSE_ATTEMPTS`] times; the library is judged on the first
/// attempt that hits the window. Every attempt checks the agreed classes:
/// never fatal, the solve succeeds, and a duplication issued after the job
/// (a missed window) is admitted. No hit in all attempts fails the scenario
/// with its own message.
fn pe01_refusal_reverse(c: &Communicator) -> Outcome {
    if level() != ThreadLevel::Serialized || c.size() < 2 {
        println!(
            "INFO not applicable at {:?} / {} rank(s) (needs MPI_THREAD_SERIALIZED and >= 2 ranks)",
            level(),
            c.size()
        );
        return Ok(());
    }
    let (r, p) = (c.rank() as usize, c.size());
    let sys = laplacian(16, true, p);
    let a = sys.matrix(r);
    let mut s = MumpsSolver::default();
    solve(c, &mut s, &sys, &a).map_err(|e| format!("setup: {e}"))?;
    let sibling = c.duplicate().map_err(e2s)?;
    let mut hit = None;
    for attempt in 1..=PE01_REVERSE_ATTEMPTS {
        s.invalidate(); // the next solve analyses and factorises (long jobs)
        let solve_done = Arc::new(AtomicBool::new(false));
        let worker = (r == 0).then(|| {
            let h = sibling.clone();
            let done = Arc::clone(&solve_done);
            std::thread::spawn(move || {
                // start the duplication while a native job of the main
                // thread runs; give up once its solve has returned
                while instance_counts().jobs_in_flight == 0 {
                    if done.load(Ordering::SeqCst) {
                        return (false, h.duplicate().map(drop));
                    }
                    std::hint::spin_loop();
                }
                (true, h.duplicate().map(drop))
            })
        });
        let main = solve(c, &mut s, &sys, &a);
        solve_done.store(true, Ordering::SeqCst);
        let (during_job, w) = match worker {
            Some(w) => w
                .join()
                .unwrap_or_else(|_| (false, Err(Error::invalid("worker panicked")))),
            // not measured here: rank 0's flag decides (Min below)
            None => (true, sibling.duplicate().map(drop)),
        };
        // (a) the precondition, agreed on its own: rank 0's measurement
        let mut flag = [u64::from(during_job)];
        c.all_reduce(&mut flag, ReduceOp::Min).map_err(e2s)?;
        let in_job = flag[0] == 1;
        let cls = agreed_classes(c, &[class(&main), class(&w)])?;
        if r == 0 {
            println!(
                "INFO pe01 reverse attempt {attempt}: duplication started during a native job: \
                 {in_job}; main {:?}, sibling duplication {:?}",
                main.as_ref().map(|_| "ok").map_err(|e| e.to_string()),
                w.as_ref().map(|_| "ok").map_err(|e| e.to_string())
            );
        }
        ensure!(
            cls[0] < 2 && cls[1] < 2,
            "attempt {attempt}: fatal outcome {cls:?}"
        );
        ensure!(cls[0] == 0, "attempt {attempt}: the solve failed: {cls:?}");
        if !in_job {
            ensure!(
                cls[1] == 0,
                "attempt {attempt}: a duplication issued after the native job was refused: {cls:?}"
            );
            continue;
        }
        hit = Some((attempt, main, cls));
        break;
    }
    let Some((attempt, main, cls)) = hit else {
        return Err(format!(
            "the duplication never started during a native job in {PE01_REVERSE_ATTEMPTS} \
             attempts (harness precondition not reached; no library outcome judged)"
        ));
    };
    ensure!(
        cls[1] == 1,
        "attempt {attempt}: the duplication was not refused as an ordinary error: {cls:?}"
    );
    not_broken(c, &sibling, "reverse")?;
    if let Some(res) = residual_of(&sys, main.as_ref().map_err(|e| e.to_string())?) {
        ensure!(res < 1e-10, "solve residual {res:e}");
    }
    sibling
        .duplicate()
        .map(drop)
        .map_err(|e| format!("retried duplication: {e}"))?;
    let g = solve(c, &mut s, &sys, &a).map_err(|e| format!("solve after: {e}"))?;
    if let Some(res) = residual_of(&sys, &g) {
        ensure!(res < 1e-10, "solve after: residual {res:e}");
    }
    s.destroy().map_err(e2s)?;
    Ok(())
}

type Scenario = (&'static str, fn(&Communicator) -> Outcome);

const SCENARIOS: &[Scenario] = &[
    (
        "mu2_01_stale_coordinates_shorter",
        mu2_01_stale_coordinates_shorter,
    ),
    (
        "mu2_01_stale_coordinates_longer",
        mu2_01_stale_coordinates_longer,
    ),
    ("mu2_02_release_collision", mu2_02_release_collision),
    (
        "mu2_04_teardown_on_broken_communicator",
        mu2_04_teardown_on_broken_communicator,
    ),
    ("mu2_05_concurrent_solvers", mu2_05_concurrent_solvers),
    ("pe01_refusal_forward", pe01_refusal_forward),
    ("pe01_refusal_reverse", pe01_refusal_reverse),
];

fn main() {
    let watchdog = std::env::var("AKANTU_MUMPS_WATCHDOG_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(240u64);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(watchdog));
        eprintln!("review_v66_regressions: watchdog ({watchdog} s) expired");
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
            "INFO review_v66_regressions: ranks={} thread_level={:?}",
            control.size(),
            level()
        );
    }
    let mut failed = 0;
    for &(name, f) in SCENARIOS {
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
        println!("review_v66_regressions: failed={failed}");
    }
    drop(work);
    drop(control);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
