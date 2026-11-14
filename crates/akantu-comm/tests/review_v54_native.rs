// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression scenarios (no upstream counterpart) of
// `Communicator::native_collective`,
// the entry point of native-library collectives (upstream
// src/solver/sparse_solver_mumps.cc passes `MPI_Comm_c2f(communicator)` to
// MUMPS as `comm_fortran`).

//! Real-MPI regressions of the native-collective entry point,
//! `harness = false`, `MPI_THREAD_SERIALIZED`:
//!
//! * `mu03_c2f_inside_serial_lock` — the Fortran handle conversion
//!   (`MPI_Comm_c2f`) is an MPI call and must run under the `Serialized`
//!   MPI lock. This binary **interposes** `MPI_Comm_c2f` (the shim's
//!   reference resolves to the definition below, which forwards to
//!   `PMPI_Comm_c2f`): once armed, it signals a worker thread that tries to
//!   take the process MPI lock (`ThreadPolicy::call`) and waits up to
//!   500 ms. If the worker gets the lock while the conversion is running,
//!   the conversion ran outside the lock
//!   (`C2F_WITH_SERIAL_LOCK_HELD_BY_OTHER_THREAD=1`).
//! * `mu02_native_job_identity` — the native header compares the job
//!   identity (`what`): ranks entering **different** native jobs fail on
//!   every rank before the library runs, and the communicator stays usable.
//!
//! Launch: `mpirun -n N review_v54_native [scenario]` (no argument: every
//! scenario, e.g. as an MPI singleton under `cargo test --features mpi`).
//! Each rank prints `V54 PASS|FAIL <scenario> rank=<r>`; the job exits 0
//! only if every rank passed.

// Test-only interposer of an MPI C function (`#[unsafe(no_mangle)]`) and
// its forwarding call; each `unsafe` states its SAFETY.
#![allow(unsafe_code)]

mod mpi_common;

use std::ffi::c_int;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, ReduceOp, SendMode};
use mpi_common::{Outcome, op_timeout, start_watchdog, world};

// ------------------------------------------------- MPI_Comm_c2f interposer

/// The next `MPI_Comm_c2f` call is probed.
static ARMED: AtomicBool = AtomicBool::new(false);
/// The probed conversion started.
static ENTERED: AtomicBool = AtomicBool::new(false);
/// The worker holds the process MPI lock.
static HELD: AtomicBool = AtomicBool::new(false);
/// Verdict of the probe: 0 none yet, 1 the conversion ran while another
/// thread held the MPI lock, 2 the lock stayed unavailable to the
/// worker for the whole wait (the conversion holds it).
static VERDICT: AtomicU8 = AtomicU8::new(0);
const VERDICT_RACE: u8 = 1;
const VERDICT_LOCKED: u8 = 2;

unsafe extern "C" {
    /// The MPI profiling entry point of the conversion (Open MPI 5 exports
    /// it; `MPI_Fint` is `int` there, as in the akantu shim's
    /// `akantu_mpi_comm_c2f` signature).
    fn PMPI_Comm_c2f(comm: mpi::ffi::MPI_Comm) -> c_int;
}

/// Interposed `MPI_Comm_c2f` (MPI standard profiling interface: a tool may
/// define the `MPI_` name and forward to `PMPI_`).
///
/// # Safety
///
/// Called by C code with a valid communicator handle, like the MPI
/// function it replaces.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn MPI_Comm_c2f(comm: mpi::ffi::MPI_Comm) -> c_int {
    if ARMED.swap(false, Ordering::SeqCst) {
        ENTERED.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        while !HELD.load(Ordering::SeqCst) && t0.elapsed() < Duration::from_millis(500) {
            std::thread::sleep(Duration::from_millis(1));
        }
        VERDICT.store(
            if HELD.load(Ordering::SeqCst) {
                VERDICT_RACE
            } else {
                VERDICT_LOCKED
            },
            Ordering::SeqCst,
        );
    }
    // SAFETY: forwards the caller's valid handle to the real conversion
    // (same signature, `PMPI_` profiling name of the same MPI library).
    unsafe { PMPI_Comm_c2f(comm) }
}

// --------------------------------------------------------------- scenarios

/// `native_collective` converts the handle under the `Serialized`
/// MPI lock (the same `ThreadPolicy::call` as the native call itself).
fn mu03_c2f_inside_serial_lock(c: &Communicator) -> Outcome {
    let env = akantu_comm::mpi_lifecycle()
        .initialize(ThreadLevel::Serialized)
        .map_err(|e| e.to_string())?;
    let policy = env.policy().clone();
    if policy.provided() != ThreadLevel::Serialized {
        println!(
            "INFO mu03: MPI provided {:?}, not Serialized: not applicable",
            policy.provided()
        );
        return Ok(());
    }
    ENTERED.store(false, Ordering::SeqCst);
    HELD.store(false, Ordering::SeqCst);
    VERDICT.store(0, Ordering::SeqCst);
    let worker = std::thread::spawn(move || -> Result<(), String> {
        let t0 = Instant::now();
        while !ENTERED.load(Ordering::SeqCst) {
            if t0.elapsed() > Duration::from_secs(60) {
                return Err("the probed conversion never started".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        policy
            .call(|| {
                HELD.store(true, Ordering::SeqCst);
                // keep the MPI lock until the interposer recorded its verdict
                let t1 = Instant::now();
                while VERDICT.load(Ordering::SeqCst) == 0 && t1.elapsed() < Duration::from_secs(60)
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
            .map_err(|e| e.to_string())
    });
    ARMED.store(true, Ordering::SeqCst);
    let r = c.native_collective("v54:mu03", Ok(()), |h| h.fortran());
    ARMED.store(false, Ordering::SeqCst);
    let w = worker
        .join()
        .unwrap_or_else(|_| Err("worker panicked".into()));
    drop(env);
    let verdict = VERDICT.load(Ordering::SeqCst);
    println!(
        "INFO mu03 rank {}: C2F_WITH_SERIAL_LOCK_HELD_BY_OTHER_THREAD={}",
        c.rank(),
        u8::from(verdict == VERDICT_RACE)
    );
    r.map_err(|e| format!("native collective failed: {e}"))?;
    w?;
    if verdict != VERDICT_LOCKED {
        return Err(format!(
            "MPI_Comm_c2f ran while another thread held the Serialized MPI lock (verdict \
             {verdict})"
        ));
    }
    Ok(())
}

/// Native job identity (defence in depth): ranks entering different native jobs fail on
/// every rank before the library runs; the communicator stays usable.
fn mu02_native_job_identity(c: &Communicator) -> Outcome {
    if c.size() < 2 {
        println!("INFO mu02_native_job_identity: not applicable on one rank");
        return Ok(());
    }
    let what = if c.rank() == 0 {
        "v54:job-a"
    } else {
        "v54:job-b"
    };
    let ran = AtomicBool::new(false);
    let r = c.native_collective(what, Ok(()), |_| ran.store(true, Ordering::SeqCst));
    let entered = ran.load(Ordering::SeqCst);
    // the same job afterwards: admitted and run on every rank
    let same = c.native_collective("v54:job-same", Ok(()), |h| h.size());
    let mut ok = [1u8];
    c.all_reduce(&mut ok, ReduceOp::Min)
        .map_err(|e| format!("communicator unusable after the mismatch: {e}"))?;
    if r.is_ok() || entered {
        return Err(format!(
            "ranks entered different native jobs (result {r:?}, library entered: {entered})"
        ));
    }
    match same {
        Ok(n) if n == c.size() => Ok(()),
        other => Err(format!("the same native job afterwards: {other:?}")),
    }
}

type Scenario = (&'static str, fn(&Communicator) -> Outcome);

const SCENARIOS: [Scenario; 2] = [
    ("mu03_c2f_inside_serial_lock", mu03_c2f_inside_serial_lock),
    ("mu02_native_job_identity", mu02_native_job_identity),
];

fn main() {
    start_watchdog(180);
    let only = std::env::args().nth(1);
    if let Some(o) = &only
        && !SCENARIOS.iter().any(|(n, _)| n == o)
    {
        eprintln!("unknown scenario {o}");
        std::process::exit(2);
    }
    let c = world(ThreadLevel::Serialized, Some(op_timeout()), SendMode::Eager);
    let mut failed = 0;
    for (name, f) in SCENARIOS {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&c)))
            .unwrap_or_else(|_| Err("panicked".into()));
        match &r {
            Ok(()) => println!("V54 PASS {name} rank={}", c.rank()),
            Err(e) => {
                failed += 1;
                println!("V54 FAIL {name} rank={}: {e}", c.rank());
            }
        }
    }
    println!("review_v54_native rank={}: failed={failed}", c.rank());
    drop(c); // last guard on the main thread: MPI_Finalize
    std::process::exit(i32::from(failed > 0));
}
