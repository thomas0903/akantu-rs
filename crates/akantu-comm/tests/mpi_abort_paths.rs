// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs acceptance scenarios (no upstream counterpart; upstream throws
// from MPI's error handler while peers wait): transport failures on real
// MPI must take the runtime abort path (`docs/ARCHITECTURE.md`), and the
// tests check the actual runtime abort and the translation of the error
// into an exception.

//! Real-MPI abort paths, one scenario per process (`harness = false`):
//!
//! ```text
//! mpirun -np N mpi_abort_paths <scenario>
//! ```
//!
//! Every scenario reaches a transport failure on at least one rank, prints
//! `EVIDENCE` lines (flushed) and calls
//! `Communicator::abort_on_transport_fatal`, i.e. `MPI_Abort(comm, 71)` —
//! or, where the thread level forbids `MPI_Abort` on the calling thread,
//! the non-MPI `_exit(71)` fallback. Peers still blocked
//! in an agreement are terminated by the abort — the runner
//! (`scripts/mpi/run-mpi-tier.sh`) checks the job's exit code (71), the
//! per-rank evidence (`scripts/mpi/check_evidence.py`), that every
//! intercepted `MPI_Abort` was legal for the thread level
//! (`scripts/mpi/abort_shim.c`, `LD_PRELOAD`), and that the job ended long
//! before the peers' own deadline. Without a scenario argument (e.g. under
//! `cargo test`) it lists the scenarios and exits 0.

mod mpi_common;

use std::io::Write;
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::mpi_lifecycle;
use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{
    CommunicationBuffer, Communicator, DataAccessor, SendMode, Synchronizer, TRANSPORT_ABORT_CODE,
    Tag,
};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Error, FatalKind, Result};
use mpi_common::{start_watchdog, world};

/// Peers' deadline: far beyond the expected abort time.
const PEER_DEADLINE: Duration = Duration::from_secs(60);
/// Deadline of operations meant to time out.
const SHORT: Duration = Duration::from_secs(2);

const SCENARIOS: &[&str] = &[
    "transport_local_result",
    "unagreeable_fatal",
    "standalone_sync_peer_missing",
    "rendezvous_naive_deadlock",
    "transport_breaks_derived",
    "mpi_error_translation",
    "funneled_worker_policy",
    "dropped_barrier_breaks_family",
    "after_header_omitted_payload",
    "payload_stall_deadline",
    "worker_abort_funneled",
    "worker_abort_serialized",
    "serialized_lock_held_abort",
    "region_abort_serialized",
    "stdout_lock_held_abort",
];

/// Deadline of the abort-path scenarios (700 ms).
const SCRATCH: Duration = Duration::from_millis(700);

fn evidence(c: &Communicator, what: &str) {
    println!("EVIDENCE rank={} {what}", c.rank());
    let _ = std::io::stdout().flush();
}

fn kind(e: &Error) -> String {
    format!("{:?}", e.fatal_kind())
}

/// Abort through the runtime policy; reaching the end is a failure.
fn abort_with(c: &Communicator, e: &Error) -> ! {
    evidence(c, &format!("aborting kind={} error=\"{e}\"", kind(e)));
    c.abort_on_transport_fatal(e);
    eprintln!(
        "rank {}: expected a transport-fatal error, got {e}",
        c.rank()
    );
    std::process::exit(2)
}

/// Scenarios failing on every rank: every rank records its evidence, a
/// healthy control communicator synchronises them (so the first abort does
/// not cut a peer's evidence short), then every rank aborts.
fn abort_all(control: &Communicator, c: &Communicator, e: &Error) -> ! {
    evidence(c, &format!("aborting kind={} error=\"{e}\"", kind(e)));
    let _ = control.barrier();
    c.abort_on_transport_fatal(e);
    eprintln!(
        "rank {}: expected a transport-fatal error, got {e}",
        c.rank()
    );
    std::process::exit(2)
}

/// Ranks that are not on the failing path record their evidence, meet the
/// failing ranks at a sync point ([`ready`]) on a healthy control
/// communicator — so no abort cuts their evidence short — and then wait on
/// it until the abort terminates them (or the deadline fails the scenario
/// with a distinct exit code).
fn wait_for_abort(control: &Communicator) -> ! {
    evidence(control, "waiting-for-abort");
    ready(control);
    let r = control.barrier();
    eprintln!("rank {}: survived the abort window: {r:?}", control.rank());
    std::process::exit(3)
}

/// Sync point of the failing ranks with the [`wait_for_abort`] ranks.
fn ready(control: &Communicator) {
    control.barrier().expect("control sync point");
}

struct Noop;
impl DataAccessor<u32> for Noop {
    fn nb_data(&self, e: &[u32], _: SynchronizationTag) -> usize {
        8 * e.len()
    }
    fn pack_data(
        &self,
        b: &mut CommunicationBuffer,
        e: &[u32],
        _: SynchronizationTag,
    ) -> Result<()> {
        for _ in e {
            b.pack(&0.0f64);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut CommunicationBuffer,
        e: &[u32],
        _: SynchronizationTag,
    ) -> Result<()> {
        for _ in e {
            b.unpack::<f64>()?;
        }
        Ok(())
    }
}

/// `phase.rs::transport_fatal_local_result_takes_the_abort_path`: rank 0's
/// local transport-fatal result is never agreed; its scope and the
/// communicator break without communicating; peers blocked in the
/// agreement are terminated by the abort.
fn transport_local_result() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let mut ph = Phases::new(&c, "op");
    if c.rank() != 0 {
        evidence(&c, "entering-agreement");
        ready(&control);
        let r = ph.agree("callback", Ok(()), Severity::Recoverable);
        eprintln!(
            "rank {}: agreement returned {r:?} instead of being aborted",
            c.rank()
        );
        std::process::exit(3);
    }
    ready(&control);
    let t0 = Instant::now();
    let local = Err(Error::fatal(
        FatalKind::Transport,
        Error::Communication("peer lost".into()),
    ));
    let r = ph.agree("callback", local, Severity::Recoverable);
    let settled = ph.settle(r).expect_err("transport-fatal");
    let fast = t0.elapsed() < Duration::from_secs(1);
    drop(ph);
    let reborn = Phases::new(&c, "again").is_broken();
    let raw = c.barrier().expect_err("refused");
    evidence(
        &c,
        &format!(
            "settled={} fast={fast} reborn_broken={reborn} raw_kind={} broken={}",
            kind(&settled),
            kind(&raw),
            c.broken().is_some()
        ),
    );
    abort_with(&c, &settled)
}

/// `phase.rs::unagreeable_fatal_outcome_takes_the_abort_path`: after a
/// recoverable agreed failure, a late agreed-kind fatal result on rank 0
/// can no longer be agreed: rank 0 takes the abort path (transport), the
/// others return the recoverable failure without communicating.
fn unagreeable_fatal() -> ! {
    let c = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let last = i32::try_from(c.size() - 1).unwrap_or(0);
    let mut ph = Phases::new(&c, "op");
    let local = if c.rank() == last {
        Err(Error::invalid("bad input"))
    } else {
        Ok(())
    };
    let pre = ph.agree("preflight", local, Severity::Recoverable);
    let late = if c.rank() == 0 {
        Err(Error::fatal(FatalKind::Agreed, Error::invalid("late")))
    } else {
        Ok(())
    };
    let t0 = Instant::now();
    let r = ph.agree("late", late, Severity::Recoverable);
    let fast = t0.elapsed() < Duration::from_secs(1);
    let settled = ph.settle(r).expect_err("failed scope");
    evidence(
        &c,
        &format!(
            "preflight_failed={} late_kind={} fast={fast} broken={}",
            pre.is_err(),
            kind(&settled),
            c.broken().is_some()
        ),
    );
    if c.rank() == 0 {
        ready(&control);
        abort_with(&c, &settled)
    }
    wait_for_abort(&control)
}

/// Prepare agreement with absent ranks on real MPI: an empty synchronizer, ranks > 0 never
/// enter the exchange; rank 0's prepare agreement reaches its deadline —
/// a transport failure, settled to `Error::Fatal { kind: Transport }`, the
/// communicator broken — and the runtime aborts.
fn standalone_sync_peer_missing() -> ! {
    let c = world(ThreadLevel::Funneled, Some(SHORT), SendMode::Eager);
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    if c.size() == 1 {
        evidence(&c, "not-applicable single-rank (no missing peer)");
        drop(control);
        drop(c); // last guards on the main thread: MPI_Finalize
        std::process::exit(0);
    }
    if c.rank() != 0 {
        wait_for_abort(&control)
    }
    ready(&control);
    let s: Synchronizer<u32> = Synchronizer::new("empty", c.clone());
    let t0 = Instant::now();
    let e = s
        .synchronize(&mut Noop, SynchronizationTag::SmmUv)
        .expect_err("deadline");
    evidence(
        &c,
        &format!(
            "kind={} after_ms={} broken={} timed_out={}",
            kind(&e),
            t0.elapsed().as_millis(),
            c.broken().is_some(),
            e.to_string().contains("timed out")
        ),
    );
    abort_with(&c, &e)
}

/// Blocking synchronous send-all before receive-all on real MPI
/// dead-locks; the deadline turns it into a transport failure on every
/// rank and the runtime aborts.
fn rendezvous_naive_deadlock() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(SHORT), SendMode::Rendezvous);
    let p = i32::try_from(c.size()).unwrap_or(1);
    let next = (c.rank() + 1) % p;
    let t0 = Instant::now();
    let e = c
        .send(&[1.0f64; 4], next, Tag(9))
        .expect_err("synchronous send-all dead-locks");
    evidence(
        &c,
        &format!(
            "kind={} after_ms={} broken={}",
            kind(&e),
            t0.elapsed().as_millis(),
            c.broken().is_some()
        ),
    );
    abort_all(&control, &c, &e)
}

/// A transport failure on a duplicate breaks the whole family: the parent
/// and a split refuse raw operations with the transport kind, new scopes
/// are born broken.
fn transport_breaks_derived() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(SHORT), SendMode::Eager);
    let dup = c.duplicate().expect("dup");
    let split = c.split(Some(0), c.rank()).expect("split").expect("member");
    let p = i32::try_from(c.size()).unwrap_or(1);
    let e = dup
        .receive_vec::<u8>((c.rank() + 1) % p, Tag(77))
        .expect_err("nobody sends");
    let parent = c.barrier().expect_err("parent refused");
    let child = split.barrier().expect_err("split refused");
    evidence(
        &c,
        &format!(
            "dup_kind={} parent_kind={} split_kind={} scope_born_broken={}",
            kind(&e),
            kind(&parent),
            kind(&child),
            Phases::new(&c, "x").is_broken()
        ),
    );
    abort_all(&control, &c, &e)
}

/// Exception translation: MPI itself rejects a call (invalid source rank,
/// `MPI_ERR_RANK`) through `MPI_ERRORS_RETURN` — under the default
/// `MPI_ERRORS_ARE_FATAL` the job would die inside MPI instead — and the
/// code becomes `Error::Fatal { kind: Transport }` with MPI's error string.
fn mpi_error_translation() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let e = c.debug_inject_mpi_error().expect_err("MPI_ERR_RANK");
    let raw = c.barrier().expect_err("broken");
    evidence(
        &c,
        &format!(
            "kind={} mentions_rank_error={} raw_kind={} message=\"{e}\"",
            kind(&e),
            e.to_string().contains("MPI_ERR_RANK"),
            kind(&raw)
        ),
    );
    abort_all(&control, &c, &e)
}

/// Thread policy on real MPI (`MPI_THREAD_FUNNELED`): a scope transferred
/// to a worker does not grant MPI thread permission — the worker's
/// agreement is refused before any MPI call, the scope breaks, and the
/// owner settles a transport-fatal error.
fn funneled_worker_policy() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let parked = Phases::new(&c, "op").park();
    let (parked, worker_err) = std::thread::scope(|s| {
        s.spawn(|| {
            let mut ph = parked.resume();
            let r = ph.agree("on-worker", Ok(()), Severity::Recoverable);
            (
                ph.park(),
                r.err().map(|e| e.to_string()).unwrap_or_default(),
            )
        })
        .join()
        .expect("worker")
    });
    let mut ph = parked.resume();
    let settled = ph
        .settle::<()>(Err(Error::Communication(worker_err.clone())))
        .expect_err("x");
    evidence(
        &c,
        &format!(
            "scope_broken={} settled_kind={} main_thread_refusal={}",
            ph.is_broken(),
            kind(&settled),
            worker_err.contains("main thread")
        ),
    );
    abort_all(&control, &c, &settled)
}

/// Single-rank runs of multi-rank scenarios: nothing to abort.
fn not_applicable(c: Communicator, control: Option<Communicator>) -> ! {
    evidence(&c, "not-applicable single-rank");
    drop(control);
    drop(c); // last guards on the main thread: MPI_Finalize
    std::process::exit(0);
}

/// Rank 0 drops an unmatched non-blocking
/// barrier on a duplicate; the drop completes it through the communicator's
/// fatal classifier, so right after the drop — before any other operation
/// could itself fail — the whole family (duplicate, parent, split) is
/// broken and refuses new MPI work. Before the fix `broken()` stayed `None`
/// and the next operation was admitted.
fn dropped_barrier_breaks_family() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(SCRATCH), SendMode::Eager);
    if c.size() == 1 {
        not_applicable(c, Some(control))
    }
    let d = c.duplicate().expect("dup");
    let split = c.split(Some(0), c.rank()).expect("split").expect("member");
    if c.rank() != 0 {
        wait_for_abort(&control)
    }
    ready(&control);
    let t0 = Instant::now();
    drop(d.ibarrier().expect("posted"));
    let dropped_ms = t0.elapsed().as_millis();
    let (dup_b, parent_b, split_b) = (
        d.broken().is_some(),
        c.broken().is_some(),
        split.broken().is_some(),
    );
    let t1 = Instant::now();
    let refused = d.barrier().expect_err("dup refused");
    let parent = c.barrier().expect_err("parent refused");
    let child = split.barrier().expect_err("split refused");
    let refused_fast = t1.elapsed() < Duration::from_millis(100)
        && [&refused, &parent, &child]
            .iter()
            .all(|e| e.to_string().contains("refused"));
    evidence(
        &c,
        &format!(
            "dropped_ms={dropped_ms} dup_broken={dup_b} parent_broken={parent_b} split_broken={split_b} \
             dup_kind={} parent_kind={} split_kind={} refused_fast={refused_fast}",
            kind(&refused),
            kind(&parent),
            kind(&child)
        ),
    );
    abort_with(&c, &refused)
}

/// Control case `after-header` (must pass): a successful checked barrier
/// on a derived communicator, then rank 1 omits a promised payload; rank 0
/// times out (transport), parent and derived are broken, the runtime
/// aborts (71).
fn after_header_omitted_payload() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(SCRATCH), SendMode::Eager);
    if c.size() == 1 {
        not_applicable(c, Some(control))
    }
    let d = c.duplicate().expect("dup");
    d.barrier().expect("checked barrier");
    if c.rank() != 0 {
        wait_for_abort(&control)
    }
    ready(&control);
    let e = d
        .receive_vec::<u8>(1, Tag(99))
        .expect_err("payload omitted");
    evidence(
        &c,
        &format!(
            "kind={} parent_broken={} derived_broken={}",
            kind(&e),
            c.broken().is_some(),
            d.broken().is_some()
        ),
    );
    abort_with(&c, &e)
}

/// Payload stall (run over TCP only): rank 1 posts a
/// 16 MiB send and stalls 3 s without MPI progress; rank 0's receive with a
/// 700 ms deadline must fail at the deadline — matching and payload share
/// one operation deadline (`MPI_Imrecv` polled) — instead of succeeding
/// after the stall (4347 ms before the fix, blocking `MPI_Mrecv`).
fn payload_stall_deadline() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(SCRATCH), SendMode::Eager);
    if c.size() == 1 {
        not_applicable(c, Some(control))
    }
    match c.rank() {
        0 => {
            ready(&control);
            let t0 = Instant::now();
            let r = c.receive_vec::<u8>(1, Tag(24));
            let ms = t0.elapsed().as_millis();
            let Err(e) = r else {
                evidence(
                    &c,
                    &format!("received-after-stall elapsed_ms={ms} bounded=false"),
                );
                std::process::exit(2)
            };
            evidence(
                &c,
                &format!("kind={} elapsed_ms={ms} bounded={}", kind(&e), ms < 2000),
            );
            abort_with(&c, &e)
        }
        1 => {
            let q = c.isend(vec![7u8; 16 << 20], 0, Tag(24)).expect("isend");
            evidence(&c, "sender-stalled");
            ready(&control);
            std::thread::sleep(Duration::from_secs(3));
            let _ = q.wait();
            eprintln!("rank 1: survived the abort window");
            std::process::exit(3)
        }
        _ => wait_for_abort(&control),
    }
}

/// A transport-fatal error as a worker would observe it.
fn worker_error() -> Error {
    Error::fatal(
        FatalKind::Transport,
        Error::Communication("peer lost (worker)".into()),
    )
}

/// Under FUNNELED a worker thread takes the
/// runtime abort path. `MPI_Abort` is not permitted off MPI's main thread,
/// so the non-MPI fallback terminates the process with 71 — the
/// `LD_PRELOAD` interception must record **no** `MPI_Abort` from a
/// non-main thread (before the fix: `thread_main=0 provided=1` on every rank).
fn worker_abort_funneled() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    evidence(&c, "worker-abort level=Funneled");
    control.barrier().expect("everyone reached the abort point");
    std::thread::scope(|s| {
        s.spawn(|| c.abort_on_transport_fatal(&worker_error()));
    });
    eprintln!("rank {}: worker abort returned", c.rank());
    std::process::exit(2)
}

/// Under SERIALIZED a worker may call `MPI_Abort` once it holds the
/// process MPI lock (legal: the interception records `provided=2`).
fn worker_abort_serialized() -> ! {
    let control = world(
        ThreadLevel::Serialized,
        Some(PEER_DEADLINE),
        SendMode::Eager,
    );
    let c = world(
        ThreadLevel::Serialized,
        Some(PEER_DEADLINE),
        SendMode::Eager,
    );
    if c.rank() != 0 {
        wait_for_abort(&control)
    }
    evidence(&c, "worker-abort level=Serialized");
    ready(&control);
    std::thread::scope(|s| {
        s.spawn(|| c.abort_on_transport_fatal(&worker_error()));
    });
    std::process::exit(2)
}

/// Under SERIALIZED, a thread stuck in an MPI call while holding the
/// process MPI lock (simulated: the main thread holds the policy lock) must
/// not block the emergency abort forever: after a bounded wait the worker's
/// abort terminates the process without MPI (no `MPI_Abort` recorded).
fn serialized_lock_held_abort() -> ! {
    let control = world(
        ThreadLevel::Serialized,
        Some(PEER_DEADLINE),
        SendMode::Eager,
    );
    let c = world(
        ThreadLevel::Serialized,
        Some(PEER_DEADLINE),
        SendMode::Eager,
    );
    if c.rank() != 0 {
        wait_for_abort(&control)
    }
    let env = mpi_lifecycle()
        .initialize(ThreadLevel::Serialized)
        .expect("guard");
    evidence(&c, "lock-held-abort level=Serialized");
    ready(&control);
    let t0 = Instant::now();
    std::thread::scope(|s| {
        let _ = env.policy().call(|| {
            // "Stuck in an MPI call" while holding the lock.
            s.spawn(|| c.abort_on_transport_fatal(&worker_error()));
            std::thread::sleep(PEER_DEADLINE);
        });
    });
    eprintln!(
        "rank 0: abort did not terminate the process within {:?}",
        t0.elapsed()
    );
    std::process::exit(2)
}

/// Under SERIALIZED the thread inside a native region
/// holds the (re-entrant) process MPI lock; its emergency abort must call
/// `MPI_Abort` at once (it holds the lock itself), not wait for the lock
/// for `ABORT_LOCK_WAIT` and fall back to process termination with "another
/// thread holds the MPI lock" (the behaviour before the fix).
fn region_abort_serialized() -> ! {
    let control = world(
        ThreadLevel::Serialized,
        Some(PEER_DEADLINE),
        SendMode::Eager,
    );
    let c = world(
        ThreadLevel::Serialized,
        Some(PEER_DEADLINE),
        SendMode::Eager,
    );
    if c.rank() != 0 {
        wait_for_abort(&control)
    }
    evidence(&c, "region-abort level=Serialized");
    ready(&control);
    let ticket = c.native_ticket("abort:region");
    if let Err(e) = ticket.verdict() {
        eprintln!("rank 0: native ticket refused: {e}");
        std::process::exit(2)
    }
    let t0 = Instant::now();
    let _ = c.native_region(ticket, "abort:region", |_| {
        c.abort_on_transport_fatal(&worker_error());
    });
    eprintln!(
        "rank 0: abort inside the native region returned after {:?}",
        t0.elapsed()
    );
    std::process::exit(2)
}

/// The emergency abort must not take the `Stdout`
/// lock. Rank 0 gets a transport failure while another thread holds the
/// stdout lock for ever (as a `println!` blocked on a full pipe would); the
/// abort must still terminate the job with `MPI_Abort` (before the fix it
/// flushed stdout first and hung until the runner's timeout).
fn stdout_lock_held_abort() -> ! {
    let control = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    let c = world(ThreadLevel::Funneled, Some(PEER_DEADLINE), SendMode::Eager);
    if c.rank() != 0 {
        wait_for_abort(&control)
    }
    let e = c.debug_inject_mpi_error().expect_err("MPI_ERR_RANK");
    evidence(
        &c,
        &format!("stdout-locked-abort kind={} level=Funneled", kind(&e)),
    );
    ready(&control);
    let (locked_tx, locked_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        std::mem::forget(std::io::stdout().lock()); // held for ever
        let _ = locked_tx.send(());
        loop {
            std::thread::park();
        }
    });
    locked_rx.recv().expect("stdout lock holder");
    let t0 = Instant::now();
    c.abort_on_transport_fatal(&e);
    eprintln!(
        "rank 0: abort returned after {:?} (expected a transport-fatal abort)",
        t0.elapsed()
    );
    std::process::exit(2)
}

fn main() {
    let arg = std::env::args().nth(1);
    let Some(name) = arg.filter(|a| SCENARIOS.contains(&a.as_str())) else {
        println!("mpi_abort_paths scenarios (expected exit code {TRANSPORT_ABORT_CODE}):");
        for s in SCENARIOS {
            println!("  {s}");
        }
        return;
    };
    start_watchdog(120);
    match name.as_str() {
        "transport_local_result" => transport_local_result(),
        "unagreeable_fatal" => unagreeable_fatal(),
        "standalone_sync_peer_missing" => standalone_sync_peer_missing(),
        "rendezvous_naive_deadlock" => rendezvous_naive_deadlock(),
        "transport_breaks_derived" => transport_breaks_derived(),
        "mpi_error_translation" => mpi_error_translation(),
        "funneled_worker_policy" => funneled_worker_policy(),
        "dropped_barrier_breaks_family" => dropped_barrier_breaks_family(),
        "after_header_omitted_payload" => after_header_omitted_payload(),
        "payload_stall_deadline" => payload_stall_deadline(),
        "worker_abort_funneled" => worker_abort_funneled(),
        "worker_abort_serialized" => worker_abort_serialized(),
        "serialized_lock_held_abort" => serialized_lock_held_abort(),
        "region_abort_serialized" => region_abort_serialized(),
        "stdout_lock_held_abort" => stdout_lock_held_abort(),
        _ => unreachable!(),
    }
}
