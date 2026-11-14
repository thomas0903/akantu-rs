// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the partitioned MyModel toy of
// test/test_model/test_common/test_model_solver (test_model_solver_my_model.hh)
// and the diagonal system of test/test_solver/test_sparse_solver_petsc.cc,
// driven through the PETSc backend; the cases themselves have no upstream
// counterpart (regressions).

//! Isolated regression cases of the fix round
//! (UD-426), one per process group:
//!
//! ```text
//! AKANTU_PETSC_CASE=<case> AKANTU_PETSC_THREAD_LEVEL=<level> mpirun -np N petsc_mpi
//! ```
//!
//! Run by `scripts/petsc/run-petsc-regressions.sh` (cases × ranks × thread
//! levels × repeats, part of the PETSc tier). Each case has its own
//! watchdog (`CASE HANG`, exit 75) because several of them hung before
//! their fix; rank 0 prints `CASE PASS <case>` or `CASE FAIL <case>`, every
//! rank prints its own failure and `INFO` lines with the measured values.
//! A case that does not apply to the rank count or thread level prints
//! `CASE PASS <case> (not applicable: …)`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy};
use akantu_core::enums::{MatrixType, TimeStepSolverType};
use akantu_core::{Array, Error, FatalKind, GlobalIdx, Idx, Real};
use akantu_linalg::petsc::{self, WorldRelation};
use akantu_linalg::{EquationLayout, EquationOwnership, TermsToAssemble};
use akantu_mesh::Mesh;
use akantu_solver::petsc_backend::PetscBackend;
use akantu_solver::{
    BackendContext, DofManager, LinearSystemBackend, ModelSolverOptions, SolverCallback,
};

use super::common::my_model::MyModel;
use super::{KspConf, Outcome, Setup, build, e2s, eq, partitioned_bar, petsc_dm, thread_level};

/// Every case: name, watchdog limit, body.
type Case = (&'static str, u64, fn(&Communicator) -> Outcome);

const CASES: [Case; 22] = [
    ("pe01_refusal_dup", 150, pe01_refusal_dup),
    ("pe01_refusal_world", 150, pe01_refusal_world),
    ("pe01_refusal_native", 150, pe01_refusal_native),
    ("pe02_deadlock", 240, pe02_deadlock),
    ("pe02_stress", 400, pe02_stress),
    ("pe04_vectors", 90, pe04_vectors),
    ("pe05_panic", 90, pe05_panic),
    ("pe06_foreign_comm", 90, pe06_foreign_comm),
    ("pe07_policy", 240, pe07_policy),
    ("pe09_world_dup", 90, pe09_world_dup),
    ("pe09_finalize_order", 90, pe09_finalize_order),
    ("pe13_command_line", 90, pe13_command_line),
    ("pt04_pending_options", 90, pt04_pending_options),
    ("pe15_world_in_region", 90, pe15_world_in_region),
    ("ud426_j_petsc", 120, ud426_j_petsc),
    ("ud426_j_petsc_read_only", 120, ud426_j_petsc_read_only),
    ("pe16_sparse_8", 90, super::test_sparse_solver_petsc),
    ("blas_pin", 30, super::blas_pin),
    // PETSc / TAO models (crates/akantu-solver/tests/petsc_pftao/cases.rs).
    (
        "gpe01_precedence",
        240,
        super::petsc_pftao::cases::gpe01_precedence,
    ),
    (
        "gpe03_ksp_setup_failure",
        90,
        super::petsc_pftao::cases::gpe03_ksp_setup_failure,
    ),
    (
        "ape01_glue_panic",
        60,
        super::petsc_pftao::cases::ape01_glue_panic,
    ),
    (
        "pt03_pending_global_option",
        120,
        super::petsc_pftao::cases::pt03_pending_global_option,
    ),
];

/// Names of every case (the runner script's default list).
pub fn names() -> Vec<&'static str> {
    CASES.iter().map(|c| c.0).collect()
}

/// Run case `name` on this process group; the process exit code.
pub fn run(name: &str) -> i32 {
    let Some(&(case, limit, f)) = CASES.iter().find(|c| c.0 == name) else {
        eprintln!(
            "petsc_mpi: unknown AKANTU_PETSC_CASE {name}; cases: {}",
            names().join(" ")
        );
        return 2;
    };
    let control = super::world();
    let rank = control.rank();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(limit));
        println!("CASE HANG {case} rank {rank}: no completion within {limit} s");
        std::process::exit(75);
    });
    let t0 = Instant::now();
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&control)))
        .unwrap_or_else(|_| Err("panicked".into()));
    if let Err(e) = &out {
        println!("rank {rank}: CASE {case}: {e}");
    }
    let mut ok = [u8::from(out.is_ok())];
    let agreed = control.all_reduce(&mut ok, ReduceOp::Min);
    let pass = agreed.is_ok() && ok[0] == 1;
    if rank == 0 {
        println!(
            "CASE {} {case} np={} level={:?} ({:.2} s){}",
            if pass { "PASS" } else { "FAIL" },
            control.size(),
            thread_level(),
            t0.elapsed().as_secs_f64(),
            agreed
                .err()
                .map_or(String::new(), |e| format!(" control: {e}"))
        );
    }
    drop(control);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    i32::from(!pass)
}

/// `CASE PASS … (not applicable)` path.
fn not_applicable(c: &Communicator, why: &str) -> Outcome {
    if c.rank() == 0 {
        println!("INFO not applicable: {why}");
    }
    Ok(())
}

fn ek<T>(r: &akantu_core::Result<T>) -> String {
    match r {
        Ok(_) => "Ok".into(),
        Err(e) => format!("Err(fatal={:?}: {e})", e.fatal_kind()),
    }
}

/// An MPI world with options of the case (deadline, reduction policy).
fn world_with(timeout: Duration, reduction: ReductionPolicy) -> Result<Communicator, String> {
    Communicator::mpi_world(MpiOptions {
        thread_level: thread_level(),
        timeout: Some(timeout),
        reduction,
        ..MpiOptions::default()
    })
    .map_err(e2s)
}

/// `[any rank true, every rank true]` of a local flag over `c`.
fn any_all(c: &Communicator, flag: bool) -> Result<(bool, bool), String> {
    let mut v = [u8::from(flag), u8::from(!flag)];
    c.all_reduce(&mut v, ReduceOp::Max).map_err(e2s)?;
    Ok((v[0] == 1, v[1] == 0))
}

/// The diagonal system of `test_sparse_solver_petsc` on `n` nodes
/// (`A_ii = 1/(1 + g_i)`, `b = 1`).
pub fn diag_dm(c: &Communicator, id: &str, n: usize) -> Result<(DofManager, Mesh), String> {
    let (mesh, gids) = partitioned_bar(c, n);
    let mut dm = petsc_dm(id, c)?;
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
    Ok((dm, mesh))
}

// ------------------------------------------------------------------

/// What competes with the solve on rank 0's second thread.
#[derive(Clone, Copy, Debug)]
enum Competitor {
    /// `duplicate()` of a sibling communicator of the solve's family.
    Duplicate,
    /// `Communicator::mpi_world()` (no header; an admission wait).
    World,
    /// A `NativeMpi` reduction on another communicator.
    NativeReduction,
}

/// `pe01_*` (case `refusal`): under `Serialized`, rank 0's second
/// thread starts a communicator creation / native reduction that the other
/// ranks join only after 1.5 s (so it stays registered in rank 0's
/// admission registry, polling); meanwhile rank 0's main thread runs a KSP
/// solve, whose native collective is refused there. The refusal must be an
/// **agreed, recoverable** error on every rank: the DOF manager and the
/// communicator family stay usable, the competing operation completes, the
/// next solve succeeds. Before the fix the refusal became
/// `Fatal{Transport}`, poisoned the manager, broke the family and the
/// duplication hung until its deadline.
fn pe01_refusal(c: &Communicator, kind: Competitor) -> Outcome {
    if thread_level() != ThreadLevel::Serialized {
        return not_applicable(
            c,
            "the admission rule exists under MPI_THREAD_SERIALIZED only",
        );
    }
    if c.size() < 2 {
        return not_applicable(c, "needs a peer that joins the competing operation late");
    }
    let work = world_with(Duration::from_secs(30), ReductionPolicy::RankOrdered)?;
    let (mut dm, _mesh) = diag_dm(&work, "pe01", 13)?;
    dm.solve("A").map_err(e2s)?;
    let sibling = match kind {
        Competitor::Duplicate => Some(work.duplicate().map_err(e2s)?),
        Competitor::NativeReduction => Some(world_with(
            Duration::from_secs(30),
            ReductionPolicy::NativeMpi,
        )?),
        Competitor::World => None,
    };
    work.barrier().map_err(e2s)?;
    let rank = work.rank();
    let h = std::thread::spawn(move || {
        if rank != 0 {
            std::thread::sleep(Duration::from_millis(1500));
        }
        let t = Instant::now();
        let r: akantu_core::Result<()> = match (kind, &sibling) {
            (Competitor::Duplicate, Some(s)) => s.duplicate().map(drop),
            (Competitor::NativeReduction, Some(s)) => {
                let mut v = [1.0_f64];
                s.all_reduce(&mut v, ReduceOp::Sum)
            }
            _ => Communicator::mpi_world(MpiOptions {
                thread_level: ThreadLevel::Serialized,
                timeout: Some(Duration::from_secs(30)),
                ..MpiOptions::default()
            })
            .map(drop),
        };
        (r, t.elapsed(), sibling)
    });
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    let during = dm.solve("A");
    let dt = t.elapsed();
    let (competitor, ct, sibling) = h.join().map_err(|_| "competitor thread panicked")?;
    let next = dm.solve("A");
    let barrier = work.barrier();
    let sib = sibling.as_ref().map(|s| s.barrier());
    println!(
        "INFO pe01 {kind:?} rank {rank}: solve during the competitor = {} ({dt:?}); \
         competitor = {} ({ct:?}); next solve = {}; barrier = {}; sibling barrier = {:?}; \
         broken = {:?}",
        ek(&during),
        ek(&competitor),
        ek(&next),
        ek(&barrier),
        sib.as_ref().map(ek),
        work.broken()
    );
    let refused_here = during.is_err();
    let (any_refused, all_refused) = any_all(c, refused_here)?;
    if rank == 0 {
        ensure!(
            refused_here,
            "the solve was not refused on rank 0 (the choreography did not overlap: {dt:?})"
        );
    }
    ensure!(
        any_refused == all_refused,
        "the refusal is not agreed (some ranks failed, others did not)"
    );
    if let Err(e) = &during {
        ensure!(
            e.fatal_kind().is_none(),
            "agreed refusal escalated to a fatal error: {e}"
        );
    }
    ensure!(
        competitor.is_ok(),
        "competing operation: {}",
        ek(&competitor)
    );
    ensure!(next.is_ok(), "next solve: {}", ek(&next));
    ensure!(barrier.is_ok(), "barrier: {}", ek(&barrier));
    ensure!(
        sib.is_none_or(|b| b.is_ok()),
        "sibling communicator unusable"
    );
    ensure!(work.broken().is_none(), "communicator broken");
    Ok(())
}

fn pe01_refusal_dup(c: &Communicator) -> Outcome {
    pe01_refusal(c, Competitor::Duplicate)
}

fn pe01_refusal_world(c: &Communicator) -> Outcome {
    pe01_refusal(c, Competitor::World)
}

fn pe01_refusal_native(c: &Communicator) -> Outcome {
    pe01_refusal(c, Competitor::NativeReduction)
}

// ------------------------------------------------------------------

/// `pe02_*` (case `deadlock`): thread A runs 20,000 KSP solves
/// with one PETSc DOF manager while thread B drops 12 other PETSc DOF
/// managers (of a sibling communicator, in SPMD order). Before the fix,
/// under `Serialized`, B's destruction held the MPI lock and waited for the
/// PETSc lock while A held the PETSc lock and waited for the MPI lock in a
/// polled agreement: both ranks hung (3/3). A progress watchdog reports a
/// hang after 60 s without a completed solve.
fn pe02_deadlock(c: &Communicator) -> Outcome {
    let c2 = c.duplicate().map_err(e2s)?;
    let (mut dm1, _m1) = diag_dm(c, "dl_main", 13)?;
    dm1.solve("A").map_err(e2s)?;
    let mut extra = Vec::new();
    for k in 0..12 {
        let (mut d, m) = diag_dm(&c2, &format!("dl_{k}"), 13)?;
        d.solve("A").map_err(e2s)?;
        drop(m);
        extra.push(d);
    }
    c.barrier().map_err(e2s)?;
    let progress = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let wd = watchdog(c.rank(), "pe02_deadlock", &progress, &done);
    let h = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        for d in extra {
            drop(d);
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    for _ in 0..20_000 {
        dm1.solve("A").map_err(e2s)?;
        progress.fetch_add(1, Ordering::SeqCst);
    }
    h.join().map_err(|_| "drop thread panicked")?;
    let _ = akantu_comm::mpi_lifecycle().poll();
    done.store(true, Ordering::SeqCst);
    let _ = wd.join();
    c2.barrier().map_err(e2s)?;
    if c.rank() == 0 {
        println!(
            "INFO pe02_deadlock: {} solves, 12 managers dropped concurrently (level {:?})",
            progress.load(Ordering::SeqCst),
            thread_level()
        );
    }
    Ok(())
}

/// Progress watchdog: exit 75 after 60 s without progress.
fn watchdog(
    rank: i32,
    case: &'static str,
    progress: &Arc<AtomicUsize>,
    done: &Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let (p, d) = (Arc::clone(progress), Arc::clone(done));
    std::thread::spawn(move || {
        let mut last = (p.load(Ordering::SeqCst), Instant::now());
        while !d.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
            let now = p.load(Ordering::SeqCst);
            if now != last.0 {
                last = (now, Instant::now());
            } else if last.1.elapsed() > Duration::from_secs(60) {
                println!("CASE HANG {case} rank {rank}: no progress for 60 s at {now}");
                std::process::exit(75);
            }
        }
    })
}

/// A small deterministic generator (identical on every rank for a common
/// seed; rank-local timing jitter from a rank-specific one).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

/// `pe02_*` randomised interleaving stress: 200 iterations; in each, three
/// fresh PETSc DOF managers of a sibling communicator (built and solved on
/// the main thread), then a worker drops them (SPMD order, rank-local
/// random delays) while the main thread runs a common random number of KSP
/// solves with its own manager. Under `Funneled` the drops are deferred to
/// the main thread (lifecycle poll at the end of each iteration).
fn pe02_stress(c: &Communicator) -> Outcome {
    let c2 = c.duplicate().map_err(e2s)?;
    let (mut main_dm, _m) = diag_dm(c, "st_main", 13)?;
    main_dm.solve("A").map_err(e2s)?;
    let progress = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let wd = watchdog(c.rank(), "pe02_stress", &progress, &done);
    let mut common = Lcg(0x5eed);
    let mut local = Lcg(0xabcd_0000 + u64::try_from(c.rank()).unwrap_or(0));
    let iterations = 200;
    for it in 0..iterations {
        let mut batch = Vec::new();
        for k in 0..3 {
            let (mut d, m) = diag_dm(&c2, &format!("st_{it}_{k}"), 13)?;
            d.solve("A").map_err(e2s)?;
            batch.push((d, m));
        }
        let delays: Vec<u64> = (0..3).map(|_| local.next() % 4).collect();
        let h = std::thread::spawn(move || {
            for ((d, m), ms) in batch.into_iter().zip(delays) {
                std::thread::sleep(Duration::from_millis(ms));
                drop(d);
                drop(m);
            }
        });
        let solves = 1 + common.next() % 12;
        for _ in 0..solves {
            main_dm.solve("A").map_err(e2s)?;
            progress.fetch_add(1, Ordering::SeqCst);
        }
        h.join().map_err(|_| "drop thread panicked")?;
        let _ = akantu_comm::mpi_lifecycle().poll();
        progress.fetch_add(1, Ordering::SeqCst);
    }
    done.store(true, Ordering::SeqCst);
    let _ = wd.join();
    c2.barrier().map_err(e2s)?;
    if c.rank() == 0 {
        println!(
            "INFO pe02_stress: {iterations} iterations, {} steps (level {:?})",
            progress.load(Ordering::SeqCst),
            thread_level()
        );
    }
    Ok(())
}

// ------------------------------------------------------------------

/// One rank's owned block of `counts` (no shared or ghost equations).
fn block_layout(counts: &[usize], rank: usize) -> Result<EquationLayout, String> {
    let first: usize = counts[..rank].iter().sum();
    let total: usize = counts.iter().sum();
    let mut l = EquationLayout::new();
    l.append_block(
        &vec![EquationOwnership::Owned; counts[rank]],
        GlobalIdx::try_from(first).map_err(|e| e.to_string())?,
        total,
        |_| None,
    )
    .map_err(e2s)?;
    Ok(l)
}

/// Assemble the diagonal `A_gg = 1 + g` of the owned rows and KSP-solve
/// `A x = 1` through the backend directly (one scope per solve).
fn backend_solve(
    c: &Communicator,
    b: &mut PetscBackend,
    layout: &EquationLayout,
    size: GlobalIdx,
) -> Result<Vec<Real>, String> {
    if b.has_matrix("A") {
        b.reset_profile("A", size).map_err(e2s)?;
    } else {
        b.new_matrix("A", MatrixType::Symmetric, size)
            .map_err(e2s)?;
    }
    let terms: Vec<(GlobalIdx, GlobalIdx, Real)> = (0..layout.nb_local())
        .filter_map(|l| layout.global(l))
        .map(|g| (g, g, 1.0 + g as f64))
        .collect();
    b.add_terms("A", &terms).map_err(e2s)?;
    let rhs = vec![1.0; layout.nb_local()];
    let mut x = vec![0.0; layout.nb_local()];
    let mut phases = Phases::new(c, "pe04");
    let mut ctx = BackendContext {
        layout,
        phases: &mut phases,
    };
    b.solve("A", &mut ctx, &rhs, &mut x).map_err(e2s)?;
    Ok(x)
}

/// The KSP solve vectors are recreated when the ownership changes at
/// a constant global size (re-partitioning). Ranks 0 and 1 exchange one
/// equation, the other ranks keep their counts: before the fix only ranks
/// 0 and 1 recreated their (collective) PETSc vectors and the job hung
/// inside `VecCreate`. After the fix the decision is reduced over the
/// ranks and the second solve is exact.
fn pe04_vectors(c: &Communicator) -> Outcome {
    let p = c.size();
    if p < 3 {
        return not_applicable(
            c,
            "needs a rank whose ownership is unchanged while others' change (≥ 3 ranks)",
        );
    }
    let r = usize::try_from(c.rank()).map_err(|e| e.to_string())?;
    let first = vec![3; p];
    let mut second = first.clone();
    second[0] = 4;
    second[1] = 2;
    let size = GlobalIdx::try_from(3 * p).map_err(|e| e.to_string())?;
    let mut b = PetscBackend::new(c).map_err(e2s)?;
    for (k, counts) in [first, second].iter().enumerate() {
        let layout = block_layout(counts, r)?;
        let x = backend_solve(c, &mut b, &layout, size)?;
        for (l, v) in x.iter().enumerate() {
            let g = layout.global(l).ok_or("no global")? as f64;
            let want = 1.0 / (1.0 + g);
            ensure!(
                (v - want).abs() <= 1e-10 * want,
                "solve {k}: x[{g}] = {v}, expected {want}"
            );
        }
    }
    if c.rank() == 0 {
        println!("INFO pe04_vectors: two solves at constant size {size}, ownership moved");
    }
    Ok(())
}

// ------------------------------------------------------------------

/// A `MyModel` with a hook run at every residual assembly (count from 1).
struct Hooked<F: FnMut(usize, &mut DofManager) -> akantu_core::Result<()>> {
    m: MyModel,
    hook: F,
    n: usize,
}

impl<F: FnMut(usize, &mut DofManager) -> akantu_core::Result<()>> SolverCallback for Hooked<F> {
    fn matrix_type(&self, id: &str) -> MatrixType {
        self.m.matrix_type(id)
    }
    fn assemble_matrix(&mut self, d: &mut DofManager, id: &str) -> akantu_core::Result<()> {
        self.m.assemble_matrix(d, id)
    }
    fn assemble_lumped_matrix(&mut self, d: &mut DofManager, id: &str) -> akantu_core::Result<()> {
        self.m.assemble_lumped_matrix(d, id)
    }
    fn assemble_residual(&mut self, d: &mut DofManager) -> akantu_core::Result<()> {
        self.n += 1;
        (self.hook)(self.n, d)?;
        self.m.assemble_residual(d)
    }
    fn assemble_residual_part(
        &mut self,
        d: &mut DofManager,
        part: &str,
    ) -> akantu_core::Result<()> {
        self.m.assemble_residual_part(d, part)
    }
    fn can_split_residual(&self) -> bool {
        self.m.can_split_residual()
    }
    fn predictor(&mut self, d: &mut DofManager) -> akantu_core::Result<()> {
        self.m.predictor(d)
    }
    fn corrector(&mut self, d: &mut DofManager) -> akantu_core::Result<()> {
        self.m.corrector(d)
    }
    fn before_solve_step(&mut self, d: &mut DofManager) -> akantu_core::Result<()> {
        self.m.before_solve_step(d)
    }
    fn after_solve_step(&mut self, d: &mut DofManager, c: bool) -> akantu_core::Result<()> {
        self.m.after_solve_step(d, c)
    }
    fn restore_last_converged_step(&mut self, d: &mut DofManager) -> akantu_core::Result<()> {
        self.m.restore_last_converged_step(d)
    }
    fn init_solver(
        &mut self,
        d: &mut DofManager,
        t: TimeStepSolverType,
    ) -> akantu_core::Result<()> {
        self.m.init_solver(d, t)
    }
    fn default_solver_type(&self) -> TimeStepSolverType {
        self.m.default_solver_type()
    }
    fn default_solver_options(&self, t: TimeStepSolverType) -> ModelSolverOptions {
        self.m.default_solver_options(t)
    }
}

/// `pe05_*` (case `panic`): the last rank's residual callback
/// panics at its 2nd call inside SNES. Every rank must leave the solve
/// promptly with an error (an agreed failure of that callback step), not
/// wait for the communicator's 20 s deadline (measured 20 s before the
/// fix, then `Fatal{Transport}` and a broken communicator).
fn pe05_panic(c: &Communicator) -> Outcome {
    let work = world_with(Duration::from_secs(20), ReductionPolicy::RankOrdered)?;
    let (model, mut ms) = build(&work, Setup::SnesCubic, true, KspConf::Direct)?;
    let armed = usize::try_from(work.rank()).map_err(|e| e.to_string())? == work.size() - 1;
    let mut cb = Hooked {
        m: model,
        hook: move |n: usize, _: &mut DofManager| {
            assert!(!(armed && n == 2), "injected panic in assemble_residual");
            Ok(())
        },
        n: 0,
    };
    let t = Instant::now();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ms.solve_step(&mut cb, None)
    }));
    let dt = t.elapsed();
    let barrier = work.barrier();
    let outcome = match &r {
        Ok(r) => ek(r),
        Err(_) => "panic escaped solve_step".to_owned(),
    };
    println!(
        "INFO pe05 rank {}: solve returned after {dt:?}: {outcome}; barrier after = {}",
        work.rank(),
        ek(&barrier)
    );
    let _ = c;
    ensure!(r.is_ok(), "the panic escaped the solve");
    ensure!(
        r.as_ref().is_ok_and(|r| r.is_err()),
        "the solve succeeded despite the panic"
    );
    ensure!(
        dt < Duration::from_secs(5),
        "the solve took {dt:?} (waited for the deadline instead of an agreed failure)"
    );
    if let Ok(Err(e)) = &r {
        ensure!(
            e.fatal_kind() != Some(FatalKind::Transport),
            "the panic became a transport failure: {e}"
        );
    }
    ensure!(barrier.is_ok(), "communicator unusable: {}", ek(&barrier));
    Ok(())
}

// ------------------------------------------------------------------

/// A PETSc object refuses a native capability of another
/// communicator (a duplicate: congruent, but not the object's). Before the
/// fix the object methods ignored the capability.
fn pe06_foreign_comm(c: &Communicator) -> Outcome {
    let other = c.duplicate().map_err(e2s)?;
    let session = petsc::Session::acquire(c).map_err(e2s)?;
    let n = 2;
    let size = n * c.size();
    let mut m = c
        .native_collective("pe06:create", Ok(()), |nc| {
            petsc::Matrix::create(&nc, c, &session, "pe06", n, size, true)
        })
        .map_err(e2s)?
        .map_err(e2s)?;
    let own = c
        .native_collective("pe06:own", Ok(()), |nc| m.assemble(&nc))
        .map_err(e2s)?;
    let foreign = other
        .native_collective("pe06:foreign", Ok(()), |nc| m.assemble(&nc))
        .map_err(e2s)?;
    println!(
        "INFO pe06 rank {}: own communicator {}; foreign capability {}",
        c.rank(),
        ek(&own),
        ek(&foreign)
    );
    ensure!(own.is_ok(), "own capability refused: {}", ek(&own));
    ensure!(
        foreign.is_err(),
        "a capability of another communicator was accepted"
    );
    drop(m);
    drop(session);
    Ok(())
}

// ------------------------------------------------------------------

/// Every combination of the binary's thread level × reduction policy
/// {rank-ordered, NativeMpi} × {KSP, SNES, TAO} solves. Before the fix,
/// `Serialized` + `NativeMpi` made every SNES/TAO solve `Fatal{Transport}`
/// at region entry (the region's own registration refused its reductions).
fn pe07_policy(c: &Communicator) -> Outcome {
    let mut table = Vec::new();
    let mut failures = Vec::new();
    // Both communicators live for the whole case: dropping the first PETSc
    // user's communicator would finalise PETSc, and PETSc 3.22 loses TAO's
    // types on a re-initialisation (documented limitation: one world per
    // process for TAO).
    let works = [ReductionPolicy::RankOrdered, ReductionPolicy::NativeMpi]
        .into_iter()
        .map(|p| world_with(Duration::from_secs(60), p).map(|w| (p, w)))
        .collect::<Result<Vec<_>, String>>()?;
    for (policy, work) in &works {
        let (policy, work) = (*policy, work.clone());
        // KSP
        let ksp = diag_dm(&work, "pe07", 13).and_then(|(mut dm, _m)| {
            dm.solve("A").map_err(e2s)?;
            dm.solve("A").map_err(e2s)
        });
        // SNES (cubic) and TAO (linear), direct inner solves.
        let mut nls = Vec::new();
        for setup in [Setup::SnesCubic, Setup::TaoLinear] {
            let r = build(&work, setup, true, KspConf::Direct).and_then(|(mut m, mut ms)| {
                ms.solve_step(&mut m, None).map_err(e2s)?;
                ms.solve_step(&mut m, None).map_err(e2s)
            });
            nls.push((setup, r));
        }
        let cell = |r: &Result<(), String>| match r {
            Ok(()) => "Ok".to_owned(),
            Err(e) => format!("Err({e})"),
        };
        table.push(format!(
            "{policy:?}: KSP {} | SNES {} | TAO {}",
            cell(&ksp),
            cell(&nls[0].1),
            cell(&nls[1].1)
        ));
        if let Err(e) = &ksp {
            failures.push(format!("{policy:?} KSP: {e}"));
        }
        for (s, r) in &nls {
            if let Err(e) = r {
                failures.push(format!("{policy:?} {s:?}: {e}"));
            }
        }
        work.barrier().map_err(e2s)?;
    }
    if c.rank() == 0 {
        for line in &table {
            println!("INFO pe07 level {:?}: {line}", thread_level());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

// ------------------------------------------------------------------

/// PETSc's world communicator is akantu's own duplicate of the
/// first PETSc user's communicator (`MPI_CONGRUENT`), alive until
/// `PetscFinalize`, never the user's handle itself (`MPI_IDENT` before the
/// fix: freed by MPI when its last akantu handle went, possibly before
/// `PetscFinalize`).
fn pe09_world_dup(c: &Communicator) -> Outcome {
    let (mut dm, _m) = diag_dm(c, "pe09", 13)?;
    dm.solve("A").map_err(e2s)?;
    let rel = c
        .native_collective("pe09:world", Ok(()), |nc| petsc::world_relation(&nc))
        .map_err(e2s)?
        .map_err(e2s)?;
    if c.rank() == 0 {
        println!("INFO pe09_world_dup: PETSC_COMM_WORLD vs the first user's communicator: {rel:?}");
    }
    ensure!(
        rel == WorldRelation::Congruent,
        "PETSC_COMM_WORLD is {rel:?} to its first user's communicator (expected an akantu-owned \
         duplicate)"
    );
    Ok(())
}

/// `pe09_*` (case `finalize_order`): PETSc first used on
/// communicator A (A's duplicate becomes `PETSC_COMM_WORLD`), then a
/// backend on B; A and its manager are dropped first, then B's manager:
/// `PetscFinalize` must run on a live world communicator (the run under
/// valgrind checks the handle use), and a later PETSc use initialises PETSc
/// again and solves.
fn pe09_finalize_order(_: &Communicator) -> Outcome {
    let a = super::world();
    let b = super::world();
    let (mut dma, ma) = diag_dm(&a, "fo_a", 13)?;
    dma.solve("A").map_err(e2s)?;
    let (mut dmb, mb) = diag_dm(&b, "fo_b", 13)?;
    dmb.solve("A").map_err(e2s)?;
    drop(dma);
    drop(ma);
    drop(a);
    let _ = akantu_comm::mpi_lifecycle().poll();
    dmb.solve("A").map_err(e2s)?;
    drop(dmb);
    drop(mb);
    let _ = akantu_comm::mpi_lifecycle().poll();
    // A new session (PETSc initialised again on a new world).
    let (mut dmc, _mc) = diag_dm(&b, "fo_c", 13)?;
    dmc.solve("A").map_err(e2s)?;
    if b.rank() == 0 {
        println!(
            "INFO pe09_finalize_order: world dropped first, finalised, re-initialised, solved"
        );
    }
    Ok(())
}

// ------------------------------------------------------------------

/// The PETSc command line differs on the last rank (`-ksp_type
/// v73_bogus`). The session start must refuse it on every rank (an agreed
/// error before `PetscInitialize`); before the fix the session started,
/// the last rank failed `Fatal{Transport}` inside `KSPSetFromOptions` and
/// the others blocked inside `KSPSolve` without deadline (job hang).
fn pe13_command_line(c: &Communicator) -> Outcome {
    if c.size() < 2 {
        return not_applicable(c, "needs a rank with a different command line");
    }
    let mut args = vec!["akantu-rs-test".to_owned()];
    if usize::try_from(c.rank()).map_err(|e| e.to_string())? == c.size() - 1 {
        args.extend(["-ksp_type".to_owned(), "v73_bogus".to_owned()]);
    }
    petsc::set_command_line(args);
    let t = Instant::now();
    let r = diag_dm(c, "pe13", 13).and_then(|(mut dm, _m)| {
        dm.backend_mut()
            .map_err(e2s)?
            .set_solver_option("A", "ksp_rtol", "1e-9")
            .map_err(e2s)?;
        dm.solve("A").map_err(e2s)
    });
    let dt = t.elapsed();
    println!(
        "INFO pe13 rank {}: backend + solve = {r:?} after {dt:?}",
        c.rank()
    );
    petsc::set_command_line(Vec::new());
    let (any, all) = any_all(c, r.is_err())?;
    ensure!(any, "a rank-specific PETSc command line was accepted");
    ensure!(all, "the refusal is not agreed");
    Ok(())
}

// ------------------------------------------------------------------

/// A global option queued with
/// `petsc::set_global_option` before PETSc starts is agreed only over the
/// caller's communicator — here each rank's serial one, the last rank with
/// another `ksp_type` (`v73_bogus`). The session start over the world must
/// refuse it on every rank, as a differing command line, with an
/// agreed `InvalidArgument` naming the queued options, before
/// `PetscInitialize`. Before the fix PETSc was initialised with options
/// that differ by rank: the last rank failed inside `KSPSetFromOptions`
/// (the private `ksp_rtol` makes the KSP read the database, UD-425) while
/// the others blocked inside the solve (job hang, the case's watchdog).
fn pt04_pending_options(c: &Communicator) -> Outcome {
    if c.size() < 2 {
        return not_applicable(c, "needs a rank with different queued options");
    }
    let last = usize::try_from(c.rank()).map_err(|e| e.to_string())? == c.size() - 1;
    let kind = if last { "v73_bogus" } else { "cg" };
    petsc::set_global_option(&Communicator::serial(), "ksp_type", kind).map_err(e2s)?;
    let t = Instant::now();
    let r = diag_dm(c, "pt04", 13).and_then(|(mut dm, _m)| {
        dm.backend_mut()
            .map_err(e2s)?
            .set_solver_option("A", "ksp_rtol", "1e-9")
            .map_err(e2s)?;
        dm.solve("A").map_err(e2s)
    });
    let dt = t.elapsed();
    println!(
        "INFO pt04 rank {}: backend + solve = {r:?} after {dt:?}",
        c.rank()
    );
    let named = matches!(&r, Err(m) if m.contains("set_global_option") && m.contains("identical on every rank"));
    let (any, all) = any_all(c, r.is_err())?;
    ensure!(any, "rank-specific queued global options were accepted");
    ensure!(all, "the refusal is not agreed");
    let (_, all_named) = any_all(c, named)?;
    ensure!(
        all_named,
        "the refusal does not name the queued options on every rank"
    );
    Ok(())
}

// ------------------------------------------------------------------

/// `Communicator::mpi_world()` called from a SNES callback (inside
/// the native region). Under `Serialized` the region's own registration
/// counts as a committed split: the call must be refused at once with an
/// ordinary error (before the fix it waited for its own registration until
/// its 20 s admission deadline and then failed `Fatal{Transport}`). Under
/// `Funneled` there is no admission rule: every rank adopts the world in
/// the same callback, which succeeds.
/// What the callback observed: (outcome, admitted, fatal, elapsed).
type Seen = (String, bool, bool, Duration);

fn pe15_world_in_region(c: &Communicator) -> Outcome {
    let level = thread_level();
    let (model, mut ms) = build(c, Setup::SnesLinear, true, KspConf::Direct)?;
    let seen: Arc<Mutex<Option<Seen>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&seen);
    let mut cb = Hooked {
        m: model,
        hook: move |n: usize, _: &mut DofManager| {
            if n == 1 {
                let t = Instant::now();
                let r = Communicator::mpi_world(MpiOptions {
                    thread_level: level,
                    timeout: Some(Duration::from_secs(20)),
                    ..MpiOptions::default()
                });
                let dt = t.elapsed();
                let fatal = r.as_ref().err().and_then(Error::fatal_kind).is_some();
                *slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some((ek(&r), r.is_ok(), fatal, dt));
            }
            Ok(())
        },
        n: 0,
    };
    let solve = ms.solve_step(&mut cb, None);
    let seen = seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .ok_or("the residual callback did not run")?;
    println!(
        "INFO pe15 rank {} level {level:?}: mpi_world in the region = {} after {:?}; solve = {}",
        c.rank(),
        seen.0,
        seen.3,
        ek(&solve)
    );
    ensure!(solve.is_ok(), "solve: {}", ek(&solve));
    match level {
        ThreadLevel::Serialized => {
            ensure!(!seen.1, "mpi_world was admitted inside the native region");
            ensure!(!seen.2, "the refusal is fatal: {}", seen.0);
            ensure!(
                seen.3 < Duration::from_secs(2),
                "the refusal took {:?} (waited for its own registration)",
                seen.3
            );
        }
        _ => {
            ensure!(seen.1, "mpi_world in the region: {}", seen.0);
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ UD-426

/// A fresh working directory shared by every rank (rank 0's process id).
fn shared_dir(c: &Communicator, tag: &str) -> Result<std::path::PathBuf, String> {
    let mut id = [u64::from(std::process::id())];
    c.broadcast(&mut id, 0).map_err(e2s)?;
    let dir = std::env::temp_dir().join(format!("akantu-{tag}-{}", id[0]));
    if c.rank() == 0 {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    c.barrier().map_err(e2s)?;
    Ok(dir)
}

/// One SNES solve (linear bar, direct KSP) in the current directory with
/// the manager-wide UD-426 switch.
fn snes_solve(c: &Communicator, write_j: bool) -> akantu_core::Result<()> {
    let (mut m, mut ms) =
        build(c, Setup::SnesLinear, true, KspConf::Direct).map_err(Error::invalid)?;
    let mut compat = ms.dof_manager.upstream_compat();
    compat.petsc_write_j_petsc = write_j;
    ms.dof_manager.set_upstream_compat(compat);
    ms.solve_step(&mut m, None)
}

/// UD-426 (D-003 B): by default no `J_petsc` file is
/// written; with the manager-wide compat switch every SNES solve writes it
/// as upstream (`J_petsc` in the working directory, Matrix Market through
/// a PETSc viewer).
fn ud426_j_petsc(c: &Communicator) -> Outcome {
    let dir = shared_dir(c, "ud426")?;
    let old = std::env::current_dir().map_err(|e| e.to_string())?;
    std::env::set_current_dir(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("J_petsc");
    let out = (|| -> Outcome {
        snes_solve(c, false).map_err(e2s)?;
        c.barrier().map_err(e2s)?;
        ensure!(!file.exists(), "J_petsc written by default");
        snes_solve(c, true).map_err(e2s)?;
        c.barrier().map_err(e2s)?;
        if c.rank() == 0 {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("J_petsc: {e}"))?;
            // PETSc's viewer indents the output of a parallel matrix
            // (upstream writes the same file).
            let mut lines = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.starts_with('%') || l.starts_with("%%"));
            let banner = lines.next().unwrap_or_default();
            ensure!(
                banner.starts_with("%%MatrixMarket matrix coordinate real general"),
                "J_petsc banner: {banner}"
            );
            let dims: Vec<usize> = lines
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|w| w.parse().ok())
                .collect();
            ensure!(
                dims.len() == 3
                    && dims[0] == super::N
                    && dims[1] == super::N
                    && dims[2] >= super::N,
                "J_petsc dimensions {dims:?}"
            );
            let entries = lines.filter(|l| !l.is_empty()).count();
            ensure!(
                entries == dims[2],
                "J_petsc: {entries} entries, header {}",
                dims[2]
            );
            println!("INFO ud426: J_petsc {dims:?}, {} bytes", text.len());
        }
        Ok(())
    })();
    std::env::set_current_dir(old).map_err(|e| e.to_string())?;
    c.barrier().map_err(e2s)?;
    if c.rank() == 0 {
        let _ = std::fs::remove_dir_all(&dir);
    }
    out
}

/// UD-426 in a read-only working directory: the compat solve fails on
/// every rank with an ordinary, agreed error (upstream's solve throws
/// there); the default solve is unaffected.
fn ud426_j_petsc_read_only(c: &Communicator) -> Outcome {
    let dir = shared_dir(c, "ud426ro")?;
    let ro = dir.join("ro");
    let mut writable = [0_u8];
    if c.rank() == 0 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir(&ro).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555))
            .map_err(|e| e.to_string())?;
        // A superuser ignores the permission bits: then the case cannot
        // produce a read-only directory.
        writable[0] = u8::from(std::fs::write(ro.join("probe"), b"x").is_ok());
    }
    c.broadcast(&mut writable, 0).map_err(e2s)?;
    if writable[0] == 1 {
        if c.rank() == 0 {
            let _ = std::fs::remove_dir_all(&dir);
        }
        return not_applicable(c, "running as a superuser: no read-only directory");
    }
    let old = std::env::current_dir().map_err(|e| e.to_string())?;
    std::env::set_current_dir(&ro).map_err(|e| e.to_string())?;
    let default = snes_solve(c, false);
    let compat = snes_solve(c, true);
    std::env::set_current_dir(old).map_err(|e| e.to_string())?;
    c.barrier().map_err(e2s)?;
    if c.rank() == 0 {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&dir);
    }
    println!(
        "INFO ud426_read_only rank {}: default = {}; compat = {}",
        c.rank(),
        ek(&default),
        ek(&compat)
    );
    ensure!(default.is_ok(), "default solve: {}", ek(&default));
    let (any, all) = any_all(c, compat.is_err())?;
    ensure!(
        any && all,
        "compat solve in a read-only directory: {}",
        ek(&compat)
    );
    if let Err(e) = &compat {
        ensure!(e.fatal_kind().is_none(), "the refusal is fatal: {e}");
    }
    Ok(())
}
