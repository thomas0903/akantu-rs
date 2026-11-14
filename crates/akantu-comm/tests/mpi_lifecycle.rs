// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/mpi_communicator_data.hh
// (MPI_Init only if not externally initialised; MPI_Finalize only then),
// python/akantu/__init__.py (mpi4py first); plus the akantu-rs lifecycle
// contract scenarios (`docs/ARCHITECTURE.md`) repeated
// on real MPI (INT-009).

//! Real-MPI lifecycle choreography, one scenario per process (MPI can be
//! initialised and finalised once per process), `harness = false`:
//!
//! ```text
//! mpirun -np N mpi_lifecycle <scenario>
//! ```
//!
//! Destructions are real `MPI_Comm_free` calls of akantu communicators
//! (duplicates of `MPI_COMM_WORLD`) queued through
//! `Lifecycle::release_handle`; `MPI_Finalize` is the real one. Each
//! scenario prints `LIFECYCLE PASS <name>` and exits 0, or prints the
//! failure and exits 1. Without an argument it lists the scenarios.

mod mpi_common;

use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use akantu_comm::lifecycle::{InitOwnership, ThreadLevel};
use akantu_comm::mpi_backend::handle_stats;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, SendMode, mpi_lifecycle};
use mpi_common::start_watchdog;

const SCENARIOS: &[&str] = &[
    "rust_first_funneled",
    "last_guard_on_worker_funneled",
    "last_guard_on_worker_multiple",
    "ar3_03_two_barriers_funneled",
    "ar3_03_main_drain_serialized",
    "ar3_03_worker_drain_serialized",
    "nested_release_funneled",
    "nested_release_serialized",
    "destruction_owns_last_guard",
    "host_first_imported",
    "host_first_funneled_worker_rejected",
    "thread_level_negotiation",
    "funneled_self_wait_on_worker_early",
    "funneled_self_wait_on_worker_late",
];

type Log = Arc<Mutex<Vec<(String, ThreadId)>>>;

fn log(l: &Log, what: &str) {
    l.lock()
        .unwrap()
        .push((what.to_owned(), std::thread::current().id()));
}

fn names(l: &Log) -> Vec<String> {
    l.lock().unwrap().iter().map(|(n, _)| n.clone()).collect()
}

fn comm(level: ThreadLevel) -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: level,
        timeout: Some(Duration::from_secs(20)),
        send_mode: SendMode::Eager,
        ..MpiOptions::default()
    })
    .expect("communicator")
}

/// Thread barrier whose waits time out (a regression fails instead of
/// hanging).
struct TimedBarrier {
    n: usize,
    state: Mutex<(usize, u64)>,
    cv: Condvar,
}

impl TimedBarrier {
    fn new(n: usize) -> Arc<Self> {
        Arc::new(Self {
            n,
            state: Mutex::new((0, 0)),
            cv: Condvar::new(),
        })
    }
    fn wait(&self) {
        let mut g = self.state.lock().unwrap();
        let generation = g.1;
        g.0 += 1;
        if g.0 == self.n {
            g.0 = 0;
            g.1 += 1;
            self.cv.notify_all();
            return;
        }
        let (g, t) = self
            .cv
            .wait_timeout_while(g, Duration::from_secs(20), |s| s.1 == generation)
            .unwrap();
        assert!(!t.timed_out() || g.1 != generation, "barrier timed out");
    }
}

macro_rules! check {
    ($cond:expr, $($fmt:tt)+) => {
        if !$cond {
            return Err(format!("{}:{}: {}", file!(), line!(), format!($($fmt)+)));
        }
    };
}

/// Common end-of-scenario checks for an akantu-owned environment.
fn owned_finalized_after_every_free(main: ThreadId) -> Result<(), String> {
    let s = handle_stats();
    check!(
        mpi_lifecycle().is_finalized(),
        "akantu did not finalise MPI"
    );
    check!(mpi::environment::is_finalized(), "MPI_Finalized is false");
    check!(
        s.finalized_on == Some(main),
        "finalised on {:?}, main is {main:?}",
        s.finalized_on
    );
    check!(
        s.freed_at_finalize == Some(s.comms_created) && s.comms_freed == s.comms_created,
        "frees before finalize: {s:?}"
    );
    println!("INFO stats {s:?}");
    Ok(())
}

fn rust_first_funneled(main: ThreadId) -> Result<(), String> {
    let c = comm(ThreadLevel::Funneled);
    let g = mpi_lifecycle()
        .initialize(ThreadLevel::Single)
        .map_err(|e| e.to_string())?;
    check!(
        g.ownership() == InitOwnership::Owned,
        "Rust-first program owns MPI"
    );
    check!(
        g.policy().on_main_thread(),
        "initialising thread is MPI's main thread"
    );
    drop(g);
    let mut v = [1u64];
    c.all_reduce(&mut v, ReduceOp::Sum)
        .map_err(|e| e.to_string())?;
    check!(v[0] == c.size() as u64, "sum");
    let d = c.duplicate().map_err(|e| e.to_string())?;
    drop(c);
    check!(
        !mpi_lifecycle().is_finalized(),
        "still in use by the duplicate"
    );
    drop(d); // last guard, main thread
    owned_finalized_after_every_free(main)?;
    let again = Communicator::mpi_world(MpiOptions::default());
    check!(
        again
            .as_ref()
            .is_err_and(|e| e.to_string().contains("finalised")),
        "no re-initialisation after finalisation: {again:?}"
    );
    Ok(())
}

/// The last guard dropped on a worker never finalises there; under
/// FUNNELED the worker cannot even free the communicator (deferred); the
/// main thread's poll frees, then finalises.
fn last_guard_on_worker(main: ThreadId, level: ThreadLevel) -> Result<(), String> {
    let c = comm(level);
    let d = c.duplicate().map_err(|e| e.to_string())?;
    drop(c);
    std::thread::scope(|s| s.spawn(move || drop(d)).join()).map_err(|_| "worker panicked")?;
    let deferred = mpi_lifecycle().deferred_len();
    check!(
        mpi_lifecycle().is_finalize_pending(),
        "finalisation pending"
    );
    check!(
        !mpi::environment::is_finalized(),
        "not finalised off the main thread"
    );
    if level == ThreadLevel::Funneled {
        check!(
            deferred == 1,
            "FUNNELED: the worker's MPI_Comm_free is deferred ({deferred})"
        );
        check!(
            handle_stats().comms_freed + 1 == handle_stats().comms_created,
            "one free pending"
        );
    } else {
        check!(
            deferred == 0,
            "MULTIPLE: the worker freed at once ({deferred})"
        );
    }
    check!(
        mpi_lifecycle().initialize(level).is_err(),
        "no new guard while the finalisation is pending"
    );
    let shut_on_worker = std::thread::scope(|s| s.spawn(|| mpi_lifecycle().shutdown()).join())
        .map_err(|_| "worker panicked")?;
    check!(
        shut_on_worker.is_err(),
        "shutdown refused off the main thread"
    );
    check!(
        mpi_lifecycle().poll().map_err(|e| e.to_string())?,
        "main thread finalises"
    );
    owned_finalized_after_every_free(main)
}

/// Destruction order, two-barrier FUNNELED reproduction with real
/// communicators: main runs D1 (taken in its batch) while worker B — holding
/// the last guard — queues D2 and its communicator's free between the two
/// barriers; finalisation must follow every destruction.
fn ar3_03_two_barriers_funneled(main: ThreadId) -> Result<(), String> {
    let lg: Log = Arc::default();
    let a = comm(ThreadLevel::Funneled);
    let b = a.duplicate().map_err(|e| e.to_string())?;
    let (b1, b2) = (TimedBarrier::new(2), TimedBarrier::new(2));
    let lc = mpi_lifecycle();
    std::thread::scope(|sc| -> Result<(), String> {
        let (l, x1, x2) = (Arc::clone(&lg), Arc::clone(&b1), Arc::clone(&b2));
        sc.spawn(move || {
            lc.release_handle(move || {
                x1.wait();
                x2.wait();
                log(&l, "destroy1");
            });
            drop(a); // queues MPI_Comm_free(a) (deferred under FUNNELED)
        })
        .join()
        .map_err(|_| "worker A panicked")?;
        check!(
            lc.deferred_len() == 2,
            "D1 and free(a) deferred: {}",
            lc.deferred_len()
        );
        let (l, x1, x2) = (Arc::clone(&lg), Arc::clone(&b1), Arc::clone(&b2));
        let worker_b = sc.spawn(move || {
            x1.wait();
            lc.release_handle(move || log(&l, "destroy2"));
            drop(b); // free(b) queued, last guard dropped: pending
            assert!(lc.is_finalize_pending());
            x2.wait();
        });
        check!(
            lc.poll().map_err(|e| e.to_string())?,
            "main finalises after every destruction"
        );
        worker_b.join().map_err(|_| "worker B panicked")?;
        Ok(())
    })?;
    check!(
        names(&lg) == ["destroy1", "destroy2"],
        "order {:?}",
        names(&lg)
    );
    check!(
        lg.lock().unwrap().iter().all(|(_, t)| *t == main),
        "destructions ran on the main thread"
    );
    check!(lc.deferred_len() == 0, "queue drained");
    owned_finalized_after_every_free(main)
}

/// Destruction order under SERIALIZED, main draining: main runs D1 (holding the
/// serialisation lock) while worker B queues D2 and drops its
/// communicator; B must not block on the lock; the last guard (main's)
/// finalises after both.
fn ar3_03_main_drain_serialized(main: ThreadId) -> Result<(), String> {
    let lg: Log = Arc::default();
    let a = comm(ThreadLevel::Serialized);
    let b = a.duplicate().map_err(|e| e.to_string())?;
    let (b1, b2) = (TimedBarrier::new(2), TimedBarrier::new(2));
    let lc = mpi_lifecycle();
    std::thread::scope(|sc| -> Result<(), String> {
        let (l, x1, x2) = (Arc::clone(&lg), Arc::clone(&b1), Arc::clone(&b2));
        let worker_b = sc.spawn(move || {
            x1.wait();
            lc.release_handle(move || log(&l, "destroy2"));
            drop(b);
            x2.wait();
        });
        let (l, x1, x2) = (Arc::clone(&lg), Arc::clone(&b1), Arc::clone(&b2));
        lc.release_handle(move || {
            x1.wait();
            x2.wait();
            log(&l, "destroy1");
        });
        worker_b.join().map_err(|_| "worker B panicked")?;
        Ok(())
    })?;
    check!(
        names(&lg) == ["destroy1", "destroy2"],
        "order {:?}",
        names(&lg)
    );
    check!(
        !lc.is_finalized(),
        "the main thread's communicator is still alive"
    );
    drop(a); // last guard, main thread
    owned_finalized_after_every_free(main)
}

/// Destruction order under SERIALIZED, a worker draining while the last guard drops on
/// another worker: the main thread's poll waits for the drainer and
/// finalises after D1, D2 and both frees.
fn ar3_03_worker_drain_serialized(main: ThreadId) -> Result<(), String> {
    let lg: Log = Arc::default();
    let a = comm(ThreadLevel::Serialized);
    let b = a.duplicate().map_err(|e| e.to_string())?;
    let (b1, b2) = (TimedBarrier::new(2), TimedBarrier::new(2));
    let lc = mpi_lifecycle();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::scope(|sc| -> Result<(), String> {
        let (l, x1, x2) = (Arc::clone(&lg), Arc::clone(&b1), Arc::clone(&b2));
        sc.spawn(move || {
            drop(a); // frees a at once (SERIALIZED allows workers)
            lc.release_handle(move || {
                x1.wait();
                x2.wait();
                log(&l, "destroy1");
            });
        });
        let (l, x1, x2) = (Arc::clone(&lg), Arc::clone(&b1), Arc::clone(&b2));
        sc.spawn(move || {
            x1.wait();
            lc.release_handle(move || log(&l, "destroy2"));
            drop(b); // last guard, on a worker
            tx.send(()).unwrap();
            x2.wait();
        });
        rx.recv_timeout(Duration::from_secs(20))
            .map_err(|_| "worker B never dropped")?;
        check!(lc.is_finalize_pending(), "pending");
        check!(lc.poll().map_err(|e| e.to_string())?, "main finalises");
        Ok(())
    })?;
    check!(
        names(&lg) == ["destroy1", "destroy2"],
        "order {:?}",
        names(&lg)
    );
    owned_finalized_after_every_free(main)
}

/// A destruction freeing a nested handle (a native object owning a
/// communicator): the nested `MPI_Comm_free` is queued and drained by the
/// same loop, before finalisation, without re-entering the SERIALIZED lock.
fn nested_release(main: ThreadId, level: ThreadLevel) -> Result<(), String> {
    let lg: Log = Arc::default();
    let a = comm(level);
    let nested = a.duplicate().map_err(|e| e.to_string())?;
    let l = Arc::clone(&lg);
    let created = handle_stats().comms_created;
    mpi_lifecycle().release_handle(move || {
        drop(nested); // queues MPI_Comm_free inside a running destruction
        log(&l, "outer");
    });
    check!(names(&lg) == ["outer"], "outer ran: {:?}", names(&lg));
    check!(
        handle_stats().comms_freed + 1 == created,
        "nested free drained by the same loop"
    );
    drop(a);
    owned_finalized_after_every_free(main)
}

/// A destruction owning the last communicator (keeping MPI alive until
/// the native object is freed) drops it while running on the main thread:
/// no self-wait, MPI finalised right after.
fn destruction_owns_last_guard(main: ThreadId) -> Result<(), String> {
    let a = comm(ThreadLevel::Funneled);
    let lg: Log = Arc::default();
    let l = Arc::clone(&lg);
    mpi_lifecycle().release_handle(move || {
        log(&l, "destroy");
        drop(a);
    });
    check!(names(&lg) == ["destroy"], "ran");
    owned_finalized_after_every_free(main)
}

/// mpi4py-first / host application (the host initialises MPI, here through
/// rsmpi): akantu imports the environment, duplicates the world, frees its
/// duplicates with its last handle and never finalises; the host does.
fn host_first_imported(_main: ThreadId) -> Result<(), String> {
    let (universe, provided) =
        mpi::initialize_with_threading(mpi::Threading::Multiple).ok_or("host init failed")?;
    let c = comm(ThreadLevel::Serialized);
    let g = mpi_lifecycle()
        .initialize(ThreadLevel::Single)
        .map_err(|e| e.to_string())?;
    check!(
        g.ownership() == InitOwnership::Imported,
        "imported environment"
    );
    check!(
        g.policy().on_main_thread(),
        "MPI_Is_thread_main on the entering thread"
    );
    drop(g);
    let mut v = [u64::try_from(c.rank()).unwrap_or(0)];
    c.all_reduce(&mut v, ReduceOp::Sum)
        .map_err(|e| e.to_string())?;
    let n = c.size() as u64;
    check!(v[0] == n * (n - 1) / 2, "sum");
    drop(c);
    let s = handle_stats();
    check!(
        s.comms_freed == s.comms_created,
        "akantu freed its duplicates: {s:?}"
    );
    check!(
        !mpi_lifecycle().is_finalized() && !mpi::environment::is_finalized(),
        "akantu never finalises an imported MPI"
    );
    check!(s.finalized_on.is_none(), "no akantu MPI_Finalize");
    // Repeated library setup in the same process is fine.
    drop(comm(ThreadLevel::Single));
    println!("INFO host provided={provided:?}");
    drop(universe); // the host finalises
    check!(mpi::environment::is_finalized(), "host finalised");
    Ok(())
}

/// An mpi4py-first program under FUNNELED entering akantu from a
/// worker thread is rejected (MPI's main thread is asked, not assumed);
/// entering from the main thread works.
fn host_first_funneled_worker_rejected(_main: ThreadId) -> Result<(), String> {
    let (universe, _) =
        mpi::initialize_with_threading(mpi::Threading::Funneled).ok_or("host init failed")?;
    let from_worker = std::thread::scope(|s| {
        s.spawn(|| {
            Communicator::mpi_world(MpiOptions::default())
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .join()
    })
    .map_err(|_| "worker panicked")?;
    check!(
        from_worker
            .as_ref()
            .is_err_and(|e| e.contains("main thread")),
        "worker entry rejected: {from_worker:?}"
    );
    let c = comm(ThreadLevel::Funneled);
    c.barrier().map_err(|e| e.to_string())?;
    // A cloned communicator does not grant use from another thread.
    let c2 = c.clone();
    let worker_use = std::thread::scope(|s| {
        s.spawn(move || c2.barrier().map_err(|e| e.to_string()))
            .join()
    })
    .map_err(|_| "worker panicked")?;
    check!(
        worker_use
            .as_ref()
            .is_err_and(|e| e.contains("main thread")),
        "FUNNELED worker call refused: {worker_use:?}"
    );
    // The refusal broke that communicator (transport class); a fresh one
    // still works for the host's cleanup.
    drop(c);
    drop(universe);
    Ok(())
}

/// Thread-level negotiation on real MPI: initialised with SINGLE, a later
/// request for MULTIPLE is refused; calls from a worker are refused.
fn thread_level_negotiation(main: ThreadId) -> Result<(), String> {
    let c = comm(ThreadLevel::Single);
    let provided = mpi::environment::threading_support();
    println!("INFO requested=Single provided={provided:?}");
    let multiple = Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Multiple,
        ..MpiOptions::default()
    });
    if provided < mpi::Threading::Multiple {
        check!(multiple.is_err(), "provided < required must be refused");
    }
    drop(multiple);
    let c2 = c.clone();
    let r = std::thread::scope(|s| {
        s.spawn(move || c2.barrier().map_err(|e| e.to_string()))
            .join()
    })
    .map_err(|_| "worker panicked")?;
    if provided <= mpi::Threading::Funneled {
        check!(r.is_err(), "worker call refused under {provided:?}");
    }
    drop(c);
    owned_finalized_after_every_free(main)
}

/// Under FUNNELED, a receive from self posted on the
/// main thread and waited on a worker is refused **deterministically** —
/// whether the message was queued before the wait (`early`) or arrives
/// 200 ms into it (`late`) — with a fatal transport error naming the
/// thread policy, as a remote request's wait is. Before the fix the early
/// case completed and the late one failed inside the progress hook.
fn funneled_self_wait_on_worker(_main: ThreadId, late: bool) -> Result<(), String> {
    use akantu_comm::Tag;
    let c = comm(ThreadLevel::Funneled);
    let me = c.rank();
    let req = c.irecv::<u64>(2, me, Tag(77)).map_err(|e| e.to_string())?;
    if !late {
        c.send(&[1u64, 2], me, Tag(77)).map_err(|e| e.to_string())?;
    }
    let started = std::time::Instant::now();
    let r = std::thread::scope(|s| {
        let h = s.spawn(move || req.wait());
        if late {
            std::thread::sleep(Duration::from_millis(200));
            // the worker was refused at the start of its wait: the family
            // is broken, so this send is refused too (not checked)
            let _ = c.send(&[1u64, 2], me, Tag(77));
        }
        h.join().map_err(|_| "worker panicked".to_owned())
    })?;
    let e = match r {
        Ok(v) => return Err(format!("worker wait completed: {v:?}")),
        Err(e) => e,
    };
    check!(
        e.fatal_kind() == Some(akantu_core::FatalKind::Transport),
        "not a fatal transport error: {e}"
    );
    check!(
        e.to_string().contains("thread"),
        "error does not name the thread policy: {e}"
    );
    println!(
        "INFO {} refusal after {:?}: {e}",
        if late { "late" } else { "early" },
        started.elapsed()
    );
    Ok(())
}

fn main() {
    let arg = std::env::args().nth(1);
    let Some(name) = arg.filter(|a| SCENARIOS.contains(&a.as_str())) else {
        println!("mpi_lifecycle scenarios:");
        for s in SCENARIOS {
            println!("  {s}");
        }
        return;
    };
    start_watchdog(120);
    let main = std::thread::current().id();
    let r = match name.as_str() {
        "rust_first_funneled" => rust_first_funneled(main),
        "last_guard_on_worker_funneled" => last_guard_on_worker(main, ThreadLevel::Funneled),
        "last_guard_on_worker_multiple" => last_guard_on_worker(main, ThreadLevel::Multiple),
        "ar3_03_two_barriers_funneled" => ar3_03_two_barriers_funneled(main),
        "ar3_03_main_drain_serialized" => ar3_03_main_drain_serialized(main),
        "ar3_03_worker_drain_serialized" => ar3_03_worker_drain_serialized(main),
        "nested_release_funneled" => nested_release(main, ThreadLevel::Funneled),
        "nested_release_serialized" => nested_release(main, ThreadLevel::Serialized),
        "destruction_owns_last_guard" => destruction_owns_last_guard(main),
        "host_first_imported" => host_first_imported(main),
        "host_first_funneled_worker_rejected" => host_first_funneled_worker_rejected(main),
        "thread_level_negotiation" => thread_level_negotiation(main),
        "funneled_self_wait_on_worker_early" => funneled_self_wait_on_worker(main, false),
        "funneled_self_wait_on_worker_late" => funneled_self_wait_on_worker(main, true),
        _ => unreachable!(),
    };
    match r {
        Ok(()) => println!("LIFECYCLE PASS {name}"),
        Err(e) => {
            println!("LIFECYCLE FAIL {name}: {e}");
            // Leave MPI to the launcher (finalised or not): a failure.
            std::process::exit(1);
        }
    }
}
