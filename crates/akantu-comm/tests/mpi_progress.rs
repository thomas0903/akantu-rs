// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs acceptance scenarios (no upstream counterpart): receive
// progress of the matched dynamic receive under real MPI,
// over TCP so that a large payload needs the sender's progress; two-thread
// choreographies under MPI_THREAD_SERIALIZED (communicator creation ×
// split, progress while waiting for a message to self).

//! Real-MPI progress scenarios, one per process (`harness = false`),
//! launched by `scripts/mpi/run-mpi-tier.sh` as
//!
//! ```text
//! mpirun --mca btl tcp,self -np N mpi_progress <scenario>
//! ```
//!
//! Each rank prints `PROGRESS PASS <scenario>` (or `PROGRESS FAIL …`) and the
//! job must exit 0. In the two payload-progress scenarios ranks other than 0 and 1
//! idle and on one rank they are not applicable (PASS); the `serialized_*`
//! scenarios run on every rank count
//! (`serialized_self_wait_progress` needs two ranks). Without an argument
//! the scenarios are listed (exit 0, e.g. under `cargo test`).

mod mpi_common;

use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, ReduceOp, ReductionPolicy, SendMode, Tag};
use akantu_core::Error;
use mpi_common::{op_timeout, start_watchdog, world, world_with_policy};

const SCENARIOS: &[&str] = &[
    "delayed_sender_completes",
    "serialized_progress",
    "serialized_crossed_dup",
    "serialized_crossed_split",
    "serialized_crossed_native",
    "serialized_crossed_mixed",
    "serialized_exclusive_rule",
    "serialized_split_vs_dup_low",
    "serialized_split_vs_dup_high",
    "serialized_split_vs_world",
    "serialized_creation_rule",
    "serialized_self_wait_progress",
    "idle_wait_cpu",
];

/// Scenarios that need two ranks (not applicable on one).
const TWO_RANKS: &[&str] = &[
    "delayed_sender_completes",
    "serialized_progress",
    "serialized_self_wait_progress",
    "idle_wait_cpu",
];

/// 16 MiB: far beyond any eager limit.
const LARGE: usize = 16 << 20;

fn pattern(i: usize) -> u8 {
    u8::try_from(i % 251).unwrap_or(0)
}

/// A delayed sender within the deadline: the matched receive and a posted
/// receive (both matching-engine slots: `MPI_Improbe` + exact-length
/// `MPI_Imrecv`) complete with the full payload once the sender resumes.
///
/// Only the choreography runs inside the operations' deadlines
/// (slow CI runners): every rank builds the payload (the sender's
/// message, the receiver's expected bytes) before a barrier, and the
/// receiver compares both payloads after both receives completed. The
/// element-by-element construction and check of 16 MiB in the debug build
/// took 7.5 s and 6.1 s under valgrind at one CPU and, with the library's
/// own packing, pushed the matched receive past its deadline on a 2-vCPU
/// CI runner although the sender's delay is 1.5 s.
fn delayed_sender_completes(c: &Communicator) -> Result<(), String> {
    let expected: Vec<u8> = (0..LARGE).map(pattern).collect();
    let first = (c.rank() == 1).then(|| expected.clone());
    c.barrier().map_err(|e| e.to_string())?;
    match (c.rank(), first) {
        (0, _) => {
            let t0 = Instant::now();
            let v = c.receive_vec::<u8>(1, Tag(30)).map_err(|e| e.to_string())?;
            let dt = t0.elapsed();
            let t1 = Instant::now();
            let rq = c
                .irecv::<u8>(LARGE, 1, Tag(31))
                .map_err(|e| e.to_string())?;
            let w = rq.wait().map_err(|e| e.to_string())?;
            let dt2 = t1.elapsed();
            if v != expected {
                return Err(format!(
                    "matched receive: wrong payload ({} bytes)",
                    v.len()
                ));
            }
            if w != expected {
                return Err(format!("posted receive: wrong payload ({} bytes)", w.len()));
            }
            println!(
                "INFO delayed_sender_completes: matched receive took {dt:?}, posted receive {dt2:?}"
            );
            Ok(())
        }
        (1, Some(data)) => {
            let t0 = Instant::now();
            let q = c.isend(data, 0, Tag(30)).map_err(|e| e.to_string())?;
            let posted = t0.elapsed();
            std::thread::sleep(Duration::from_millis(1500));
            q.wait().map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(500));
            let t1 = Instant::now();
            c.send(&expected, 0, Tag(31)).map_err(|e| e.to_string())?;
            println!(
                "INFO delayed_sender_completes: isend posted after {posted:?}, second send took {:?}",
                t1.elapsed()
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Same-process progress under SERIALIZED: while a worker thread's
/// receive waits for a stalled 16 MiB payload, the main thread's MPI calls
/// on another communicator must still get the process MPI lock promptly —
/// the receive polls `MPI_Imrecv`'s request and releases the lock between
/// polls (the earlier code held it in a blocking `MPI_Mrecv` for the whole stall).
fn serialized_progress(a: &Communicator) -> Result<(), String> {
    let b = a.duplicate().map_err(|e| e.to_string())?;
    let r = match a.rank() {
        0 => std::thread::scope(|s| {
            let worker = s.spawn(|| a.receive_vec::<u8>(1, Tag(32)));
            // The worker has matched the message (its rendezvous header
            // arrived) and waits for the payload.
            std::thread::sleep(Duration::from_millis(400));
            let mut worst = Duration::ZERO;
            for _ in 0..10 {
                let t = Instant::now();
                b.iprobe::<u8>(Some(1), Tag(99))
                    .map_err(|e| e.to_string())?;
                worst = worst.max(t.elapsed());
                std::thread::sleep(Duration::from_millis(50));
            }
            let v = worker
                .join()
                .map_err(|_| "receiver panicked".to_owned())?
                .map_err(|e| e.to_string())?;
            println!("INFO serialized_progress: worst main-thread MPI call {worst:?}");
            if v.len() != LARGE {
                return Err(format!("received {} bytes", v.len()));
            }
            if worst > Duration::from_millis(250) {
                return Err(format!(
                    "a main-thread MPI call waited {worst:?} for the MPI lock held by the receive"
                ));
            }
            Ok(())
        }),
        1 => {
            let q = a
                .isend(vec![3u8; LARGE], 0, Tag(32))
                .map_err(|e| e.to_string())?;
            // Stall without any MPI call: the payload needs our progress.
            std::thread::sleep(Duration::from_secs(2));
            q.wait().map(|_| ()).map_err(|e| e.to_string())
        }
        _ => Ok(()),
    };
    b.barrier().map_err(|e| e.to_string())?;
    r
}

// ------------------------------------------- staggered operations

/// Delay of the second thread in the crossed choreography.
const STAGGER: Duration = Duration::from_secs(1);
/// Per-operation deadline of the staggered scenarios (the native calls and
/// `MPI_Comm_split` ignore it after their header: a deadlock there hangs
/// until the watchdog).
const OP_TIMEOUT: Duration = Duration::from_secs(5);
/// A crossed round must finish well before the per-operation deadline.
const ROUND_BOUND: Duration = Duration::from_secs(4);
const ROUNDS: usize = 3;

fn serialized(policy: ReductionPolicy) -> Communicator {
    world_with_policy(ThreadLevel::Serialized, Some(OP_TIMEOUT), policy)
}

/// The staggered choreography: two threads per rank; thread A of an
/// even rank starts `op(x)` at t = 0 and thread B `op(y)` at t = STAGGER,
/// odd ranks the reverse (before the fix: native all-reduce deadlocked in 5/6
/// runs, `duplicate()` in 2/4). Returns the results in `(x, y)` order and
/// the elapsed time.
fn crossed<R: Send>(
    x: &Communicator,
    y: &Communicator,
    op: impl Fn(&Communicator) -> R + Sync,
) -> Result<(R, R, Duration), String> {
    let even = x.rank() % 2 == 0;
    let (first, second) = if even { (x, y) } else { (y, x) };
    let t0 = Instant::now();
    let (a, b) = std::thread::scope(|s| {
        let ha = s.spawn(|| op(first));
        let hb = s.spawn(|| {
            std::thread::sleep(STAGGER);
            op(second)
        });
        (ha.join(), hb.join())
    });
    let dt = t0.elapsed();
    let a = a.map_err(|_| "thread A panicked".to_owned())?;
    let b = b.map_err(|_| "thread B panicked".to_owned())?;
    Ok(if even { (a, b, dt) } else { (b, a, dt) })
}

/// Every rank reached the same outcome (`ok`) on one communicator.
fn agreed(control: &Communicator, ok: bool) -> Result<bool, String> {
    let mut lo = [u8::from(ok)];
    let mut hi = lo;
    control
        .all_reduce(&mut lo, ReduceOp::Min)
        .map_err(|e| e.to_string())?;
    control
        .all_reduce(&mut hi, ReduceOp::Max)
        .map_err(|e| e.to_string())?;
    if lo != hi {
        return Err("ranks disagree on the outcome of one collective".into());
    }
    Ok(lo[0] == 1)
}

/// An error that must be the agreed, non-fatal refusal (or its agreed echo
/// on the other ranks): never a transport failure.
fn non_fatal<T>(r: &Result<T, Error>, what: &str) -> Result<(), String> {
    match r {
        Err(e) if e.fatal_kind().is_some() => Err(format!("{what}: fatal error {e}")),
        _ => Ok(()),
    }
}

fn healthy(cs: &[&Communicator]) -> Result<(), String> {
    for c in cs {
        if let Some(b) = c.broken() {
            return Err(format!("communicator broken: {b}"));
        }
        c.barrier().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn bounded(dt: Duration, what: &str) -> Result<(), String> {
    if dt > ROUND_BOUND {
        return Err(format!("{what}: crossed round took {dt:?}"));
    }
    Ok(())
}

/// Crossed `duplicate()` on two communicators always completes
/// (polled `MPI_Comm_idup`, no lock held while waiting for peers).
fn serialized_crossed_dup(c: &Communicator) -> Result<(), String> {
    let x = c.duplicate().map_err(|e| e.to_string())?;
    let y = x.duplicate().map_err(|e| e.to_string())?;
    for round in 0..ROUNDS {
        let (dx, dy, dt) = crossed(&x, &y, Communicator::duplicate)?;
        let (dx, dy) = (
            dx.map_err(|e| e.to_string())?,
            dy.map_err(|e| e.to_string())?,
        );
        bounded(dt, "duplicate")?;
        healthy(&[&dx, &dy, &x, &y])?;
        println!("INFO serialized_crossed_dup round {round}: {dt:?}");
    }
    Ok(())
}

/// Crossed `split()` (blocking `MPI_Comm_split`, an exclusive
/// operation) never deadlocks: each split either succeeds or fails with the
/// agreed non-fatal refusal, identically on every rank.
fn serialized_crossed_split(c: &Communicator) -> Result<(), String> {
    let x = c.duplicate().map_err(|e| e.to_string())?;
    let y = x.duplicate().map_err(|e| e.to_string())?;
    let p = c.size();
    let color = u32::try_from(c.rank() % 2).map_err(|e| e.to_string())?;
    let want = (p + 1 - usize::try_from(color).unwrap_or(0)) / 2;
    for round in 0..ROUNDS {
        let (sx, sy, dt) = crossed(&x, &y, |k| k.split(Some(color), 0))?;
        bounded(dt, "split")?;
        let mut oks = Vec::new();
        for (r, what) in [(&sx, "split(x)"), (&sy, "split(y)")] {
            non_fatal(r, what)?;
            if let Ok(s) = r {
                let s = s.as_ref().ok_or(format!("{what}: no communicator"))?;
                if s.size() != want {
                    return Err(format!("{what}: size {} instead of {want}", s.size()));
                }
            }
            oks.push(agreed(c, r.is_ok())?);
        }
        healthy(&[&x, &y])?;
        println!("INFO serialized_crossed_split round {round}: ok(x,y)={oks:?} {dt:?}");
    }
    Ok(())
}

fn native_sum(k: &Communicator) -> Result<f64, Error> {
    let mut v = [f64::from(k.rank() + 1)];
    k.all_reduce(&mut v, ReduceOp::Sum)?;
    Ok(v[0])
}

fn expected_sum(p: usize) -> f64 {
    let p = u32::try_from(p).unwrap_or(0);
    f64::from(p * (p + 1) / 2)
}

/// Crossed native all-reductions (blocking `MPI_Allreduce`, an
/// exclusive operation) never deadlock: each one either gives the right
/// sum or fails with the agreed non-fatal refusal, identically on every
/// rank.
fn serialized_crossed_native(c: &Communicator) -> Result<(), String> {
    let x = serialized(ReductionPolicy::NativeMpi);
    let y = x.duplicate().map_err(|e| e.to_string())?;
    let want = expected_sum(c.size());
    for round in 0..ROUNDS {
        let (rx, ry, dt) = crossed(&x, &y, native_sum)?;
        bounded(dt, "native all-reduce")?;
        let mut oks = Vec::new();
        for (r, what) in [(&rx, "all_reduce(x)"), (&ry, "all_reduce(y)")] {
            non_fatal(r, what)?;
            if let Ok(v) = r
                && *v != want
            {
                return Err(format!("{what}: {v} instead of {want}"));
            }
            oks.push(agreed(c, r.is_ok())?);
        }
        healthy(&[&x, &y])?;
        println!("INFO serialized_crossed_native round {round}: ok(x,y)={oks:?} {dt:?}");
    }
    Ok(())
}

/// A native all-reduction crossed with a rank-ordered one (polled,
/// not exclusive) always completes on both.
fn serialized_crossed_mixed(c: &Communicator) -> Result<(), String> {
    let x = serialized(ReductionPolicy::NativeMpi);
    let y = serialized(ReductionPolicy::RankOrdered);
    let want = expected_sum(c.size());
    for round in 0..ROUNDS {
        let (rx, ry, dt) = crossed(&x, &y, native_sum)?;
        bounded(dt, "mixed all-reduce")?;
        let (vx, vy) = (
            rx.map_err(|e| e.to_string())?,
            ry.map_err(|e| e.to_string())?,
        );
        if vx != want || vy != want {
            return Err(format!("sums {vx}, {vy} instead of {want}"));
        }
        healthy(&[&x, &y])?;
        println!("INFO serialized_crossed_mixed round {round}: {dt:?}");
    }
    Ok(())
}

/// The exclusivity rule made deterministic: rank 0's thread A
/// starts a native all-reduce on `x` that its peers join only 2 s later;
/// at 0.5 s every rank's thread B starts one on `y`. Rank 0 refuses `y`
/// (its `x` is in flight), so `y` fails on **every** rank with the agreed
/// non-fatal error (rank 0's names the rule), while `x` completes. On one
/// rank nothing overlaps and both succeed.
fn serialized_exclusive_rule(c: &Communicator) -> Result<(), String> {
    let x = serialized(ReductionPolicy::NativeMpi);
    let y = x.duplicate().map_err(|e| e.to_string())?;
    let want = expected_sum(c.size());
    let (rx, ry) = std::thread::scope(|s| {
        let ha = s.spawn(|| {
            if x.rank() != 0 {
                std::thread::sleep(Duration::from_secs(2));
            }
            native_sum(&x)
        });
        let hb = s.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            native_sum(&y)
        });
        (ha.join(), hb.join())
    });
    let rx = rx.map_err(|_| "thread A panicked".to_owned())?;
    let ry = ry.map_err(|_| "thread B panicked".to_owned())?;
    let vx = rx.map_err(|e| format!("x: {e}"))?;
    if vx != want {
        return Err(format!("x: {vx} instead of {want}"));
    }
    if c.size() == 1 {
        let vy = ry.map_err(|e| format!("y: {e}"))?;
        if vy != want {
            return Err(format!("y: {vy} instead of {want}"));
        }
    } else {
        non_fatal(&ry, "y")?;
        let e = ry
            .err()
            .ok_or("y succeeded although rank 0's x was in flight")?;
        if c.rank() == 0
            && !e
                .to_string()
                .contains("refused under MPI_THREAD_SERIALIZED")
        {
            return Err(format!("rank 0 did not name the exclusivity rule: {e}"));
        }
        println!("INFO serialized_exclusive_rule: y refused: {e}");
    }
    healthy(&[&x, &y])?;
    // Sequential exclusive operations are admitted.
    let (a, b) = (native_sum(&y), native_sum(&x));
    if a.as_ref().ok() != Some(&want) || b.as_ref().ok() != Some(&want) {
        return Err(format!("sequential native all-reduce: {a:?}, {b:?}"));
    }
    Ok(())
}

// ------------------------------------------- creation × split scenarios

/// Rounds of the creation × split choreographies (8; before the admission
/// rule the split × duplicate one hung in rounds 0–4 in 10/10 runs at 2 ranks).
const CM3_ROUNDS: usize = 8;
/// Odd ranks start both threads this much later.
const CM3_LAG: Duration = Duration::from_millis(300);
/// Split-thread offsets of the odd ranks, per round (µs; negative: the
/// split thread starts first).
const CM3_JITTER_US: [i64; 8] = [0, 0, 20, -20, 60, -60, 150, -150];

/// The creation thread's operation in the creation × split choreographies.
#[derive(Clone, Copy, Debug)]
enum Creation<'a> {
    /// `duplicate()` of the given communicator.
    Dup(&'a Communicator),
    /// `Communicator::mpi_world()` (duplication of `MPI_COMM_WORLD`, the
    /// lowest context id, without a header).
    World,
}

fn create(c: Creation<'_>) -> Result<(), Error> {
    match c {
        Creation::Dup(k) => k.duplicate().map(|_| ()),
        Creation::World => Communicator::mpi_world(akantu_comm::MpiOptions {
            thread_level: ThreadLevel::Serialized,
            timeout: Some(OP_TIMEOUT),
            ..akantu_comm::MpiOptions::default()
        })
        .map(|_| ()),
    }
}

/// Creation × split choreography: per round, thread D runs `creation` and
/// thread S `split_comm.split()`; even ranks start both at once, odd ranks
/// both [`CM3_LAG`] later with the split offset by the round's jitter. Before
/// the fix the blocking `MPI_Comm_split` waited (Open MPI context-id
/// order) for the lower-id `MPI_Comm_idup` whose peer thread could not get
/// the `Serialized` lock held by that peer's split: a silent hang. Now each
/// operation completes or fails with the agreed non-fatal refusal,
/// identically on every rank, well within the deadline.
fn split_vs_creation(
    c: &Communicator,
    creation: Creation<'_>,
    split_comm: &Communicator,
    name: &str,
) -> Result<(), String> {
    let even = c.rank() % 2 == 0;
    let mut admitted = [0usize; 2];
    for round in 0..CM3_ROUNDS {
        c.barrier().map_err(|e| e.to_string())?;
        let j = CM3_JITTER_US[round % CM3_JITTER_US.len()];
        let gate = std::sync::Barrier::new(2);
        let start = |first: bool| {
            if !even {
                std::thread::sleep(CM3_LAG);
                gate.wait();
                if first == (j < 0) && j != 0 {
                    std::thread::sleep(Duration::from_micros(j.unsigned_abs()));
                }
            }
        };
        let t0 = Instant::now();
        let (rd, rs) = std::thread::scope(|s| {
            let hd = s.spawn(|| {
                start(true);
                create(creation)
            });
            let hs = s.spawn(|| {
                start(false);
                split_comm.split(Some(0), 0).map(|_| ())
            });
            (hd.join(), hs.join())
        });
        let dt = t0.elapsed();
        let rd = rd.map_err(|_| "creation thread panicked".to_owned())?;
        let rs = rs.map_err(|_| "split thread panicked".to_owned())?;
        non_fatal(&rd, "creation")?;
        non_fatal(&rs, "split")?;
        bounded(dt, name)?;
        let ok = [agreed(c, rd.is_ok())?, agreed(c, rs.is_ok())?];
        for (n, k) in admitted.iter_mut().zip(ok) {
            *n += usize::from(k);
        }
        println!(
            "INFO {name} round {round} jitter={j}us: creation={:?} split={:?} after {dt:?}",
            rd.as_ref().map_err(ToString::to_string),
            rs.as_ref().map_err(ToString::to_string),
        );
    }
    healthy(&[c, split_comm])?;
    println!(
        "INFO {name}: admitted creation {}/{CM3_ROUNDS}, split {}/{CM3_ROUNDS}",
        admitted[0], admitted[1]
    );
    // Sequential creations and splits are admitted.
    create(creation).map_err(|e| format!("sequential creation: {e}"))?;
    split_comm
        .split(Some(0), 0)
        .map_err(|e| format!("sequential split: {e}"))?;
    Ok(())
}

/// The failing case: duplicate the older communicator
/// `x` (lower context id) while splitting the newer `y`.
fn serialized_split_vs_dup_low(c: &Communicator) -> Result<(), String> {
    let x = c.duplicate().map_err(|e| e.to_string())?;
    let y = x.duplicate().map_err(|e| e.to_string())?;
    split_vs_creation(c, Creation::Dup(&x), &y, "serialized_split_vs_dup_low")
}

/// The control: split the older communicator, duplicate
/// the newer one.
fn serialized_split_vs_dup_high(c: &Communicator) -> Result<(), String> {
    let x = c.duplicate().map_err(|e| e.to_string())?;
    let y = x.duplicate().map_err(|e| e.to_string())?;
    split_vs_creation(c, Creation::Dup(&y), &x, "serialized_split_vs_dup_high")
}

/// `mpi_world()` (no header; `MPI_COMM_WORLD` has the lowest
/// context id) while splitting `y` (6/6 hung before the fix).
fn serialized_split_vs_world(c: &Communicator) -> Result<(), String> {
    let x = c.duplicate().map_err(|e| e.to_string())?;
    let y = x.duplicate().map_err(|e| e.to_string())?;
    split_vs_creation(c, Creation::World, &y, "serialized_split_vs_world")
}

/// The creation rule made deterministic.
///
/// (a) Rank 0's thread D starts `x.duplicate()` at t = 0, its peers only
///     at 1.5 s; every rank's thread S splits `y` at 0.5 s. Rank 0 cannot
///     commit the split (its duplication is in flight), so the split fails
///     on **every** rank with the agreed non-fatal refusal (rank 0's names
///     the rule) and the duplication completes. On one rank nothing
///     overlaps: both succeed.
/// (b) Every rank's thread S starts splitting `y` at t = 0 except rank 0,
///     which joins at 1 s; every rank's thread W calls `mpi_world()` at
///     0.3 s. An uncommitted split does not hold `mpi_world()` back: it
///     completes long before 1 s, and the split then commits and succeeds.
fn serialized_creation_rule(c: &Communicator) -> Result<(), String> {
    let x = c.duplicate().map_err(|e| e.to_string())?;
    let y = x.duplicate().map_err(|e| e.to_string())?;
    let p = c.size();
    // (a)
    let (rd, rs) = std::thread::scope(|s| {
        let hd = s.spawn(|| {
            if c.rank() != 0 {
                std::thread::sleep(Duration::from_millis(1500));
            }
            x.duplicate().map(|_| ())
        });
        let hs = s.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            y.split(Some(0), 0).map(|_| ())
        });
        (hd.join(), hs.join())
    });
    let rd = rd.map_err(|_| "duplicate thread panicked".to_owned())?;
    let rs = rs.map_err(|_| "split thread panicked".to_owned())?;
    rd.map_err(|e| format!("(a) duplicate: {e}"))?;
    if p == 1 {
        rs.map_err(|e| format!("(a) split on one rank: {e}"))?;
    } else {
        non_fatal(&rs, "(a) split")?;
        let e = rs
            .err()
            .ok_or("(a) split succeeded although rank 0's duplication was in flight")?;
        if c.rank() == 0
            && !e
                .to_string()
                .contains("refused under MPI_THREAD_SERIALIZED")
        {
            return Err(format!("(a) rank 0 did not name the admission rule: {e}"));
        }
        println!("INFO serialized_creation_rule (a): split refused: {e}");
    }
    healthy(&[c, &x, &y])?;
    // (b)
    let t0 = Instant::now();
    let (rw, rs) = std::thread::scope(|s| {
        let hw = s.spawn(|| {
            std::thread::sleep(Duration::from_millis(300));
            let r = create(Creation::World);
            (r, t0.elapsed())
        });
        let hs = s.spawn(|| {
            if c.rank() == 0 {
                std::thread::sleep(Duration::from_secs(1));
            }
            y.split(Some(0), 0).map(|k| k.map(|k| k.size()))
        });
        (hw.join(), hs.join())
    });
    let (rw, tw) = rw.map_err(|_| "world thread panicked".to_owned())?;
    let rs = rs.map_err(|_| "split thread panicked".to_owned())?;
    rw.map_err(|e| format!("(b) mpi_world: {e}"))?;
    if tw > Duration::from_millis(900) {
        return Err(format!(
            "(b) mpi_world waited for an uncommitted split: done after {tw:?}"
        ));
    }
    let size = rs.map_err(|e| format!("(b) split: {e}"))?;
    if size != Some(p) {
        return Err(format!("(b) split size {size:?} instead of {p}"));
    }
    println!("INFO serialized_creation_rule (b): mpi_world done after {tw:?}, split ok");
    healthy(&[c, &x, &y])
}

// ------------------------------------------- self-message progress

/// A thread waiting for a message to **self** still
/// runs the process's matching engine and MPI progress. Rank 0 posts two
/// receives from rank 1 (1 KiB and 16 MiB), then its main thread waits
/// 2.5 s for a self-message that a second thread sends late (standing for
/// host code that makes no akantu call). Rank 1's synchronous send (needs
/// rank 0's matching engine to match the posted receive) and the
/// completion of its 16 MiB send (rendezvous: rank 0 must match it, and
/// over TCP read the payload — MPI progress once the receive is matched)
/// must complete long before that. Before the fix both waited for the
/// self-message (the self channel blocked on a condition variable without
/// any MPI call). The timed part involves no conversion of the payload.
fn serialized_self_wait_progress(c: &Communicator) -> Result<(), String> {
    const SYNC: Tag = Tag(40);
    const SELF: Tag = Tag(41);
    const BIG: Tag = Tag(42);
    const SELF_DELAY: Duration = Duration::from_millis(2500);
    const BOUND: Duration = Duration::from_millis(1500);
    let e = |e: Error| e.to_string();
    let r = match c.rank() {
        0 => {
            let small = c.irecv::<u8>(1024, 1, SYNC).map_err(e)?;
            let big = c.irecv::<u8>(LARGE, 1, BIG).map_err(e)?;
            c.barrier().map_err(e)?;
            let t0 = Instant::now();
            let got = std::thread::scope(|s| {
                s.spawn(|| {
                    std::thread::sleep(SELF_DELAY);
                    c.send(&[9u8; 16], 0, SELF)
                });
                c.receive_vec::<u8>(0, SELF)
            });
            println!(
                "INFO serialized_self_wait_progress: self-message after {:?}",
                t0.elapsed()
            );
            if got.map_err(e)? != vec![9u8; 16] {
                return Err("wrong self-message".into());
            }
            if small.wait().map_err(e)? != vec![7u8; 1024] {
                return Err("1 KiB posted receive: wrong payload".into());
            }
            let v = big.wait().map_err(e)?;
            if v.len() != LARGE || v.iter().any(|&b| b != 5) {
                return Err("16 MiB posted receive: wrong payload".into());
            }
            Ok(())
        }
        1 => {
            c.barrier().map_err(e)?;
            // Rank 0 is in its self-message wait by now.
            std::thread::sleep(Duration::from_millis(200));
            let t0 = Instant::now();
            c.send_mode(&[7u8; 1024], 0, SYNC, SendMode::Rendezvous)
                .map_err(e)?;
            let d_sync = t0.elapsed();
            let q = c.isend(vec![5u8; LARGE], 0, BIG).map_err(e)?;
            let t1 = Instant::now();
            q.wait().map_err(e)?;
            let d_big = t1.elapsed();
            println!(
                "INFO serialized_self_wait_progress: synchronous send {d_sync:?}, 16 MiB send {d_big:?}"
            );
            if d_sync > BOUND || d_big > BOUND {
                return Err(format!(
                    "rank 0's self-message wait made no progress: synchronous send {d_sync:?}, \
                     16 MiB send {d_big:?} (bound {BOUND:?})"
                ));
            }
            Ok(())
        }
        _ => c.barrier().map_err(e),
    };
    c.barrier().map_err(|e| e.to_string())?;
    r
}

// ------------------------------------------- latency

/// CPU time (user + system) of the calling thread, from
/// `/proc/thread-self/stat` (Linux; clock ticks of 10 ms).
fn thread_cpu() -> Result<Duration, String> {
    let stat = std::fs::read_to_string("/proc/thread-self/stat").map_err(|e| e.to_string())?;
    let rest = stat
        .rsplit_once(')')
        .ok_or("malformed /proc/thread-self/stat")?
        .1;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let ticks = |i: usize| -> Result<u64, String> {
        f.get(i)
            .ok_or("short /proc/thread-self/stat")?
            .parse::<u64>()
            .map_err(|e| e.to_string())
    };
    // Fields 14 (utime) and 15 (stime); `rest` starts at field 3.
    Ok(Duration::from_millis(10 * (ticks(11)? + ticks(12)?)))
}

/// The poll pacing must not burn a core while a wait is
/// idle. Rank 0 waits 2 s for a message (matching-engine wait) and 2 s in
/// a barrier (polled collective) while rank 1 sleeps; rank 0's thread may
/// use at most a quarter of the wall time (a busy poll uses all of it;
/// the paced wait yields for 20 ms, then polls every ~300 µs).
fn idle_wait_cpu(c: &Communicator) -> Result<(), String> {
    const IDLE: Duration = Duration::from_secs(2);
    let e = |e: Error| e.to_string();
    match c.rank() {
        0 => {
            for what in ["receive", "barrier"] {
                let (t0, cpu0) = (Instant::now(), thread_cpu()?);
                if what == "receive" {
                    c.receive_vec::<u8>(1, Tag(50)).map_err(e)?;
                } else {
                    c.barrier().map_err(e)?;
                }
                let (wall, cpu) = (t0.elapsed(), thread_cpu()? - cpu0);
                println!("INFO idle_wait_cpu: idle {what} wait {wall:?}, thread CPU {cpu:?}");
                if wall < IDLE - Duration::from_millis(100) {
                    return Err(format!("{what}: the wait was not idle ({wall:?})"));
                }
                if cpu * 4 > wall {
                    return Err(format!("{what}: {cpu:?} CPU in a {wall:?} idle wait"));
                }
            }
            Ok(())
        }
        1 => {
            std::thread::sleep(IDLE);
            c.send(&[1u8; 8], 0, Tag(50)).map_err(e)?;
            std::thread::sleep(IDLE);
            c.barrier().map_err(e)
        }
        _ => c.barrier().map_err(e),
    }
}

fn main() {
    let arg = std::env::args().nth(1);
    let Some(name) = arg.filter(|a| SCENARIOS.contains(&a.as_str())) else {
        println!("mpi_progress scenarios (run over TCP: --mca btl tcp,self):");
        for s in SCENARIOS {
            println!("  {s}");
        }
        return;
    };
    start_watchdog(120);
    let level = if name.starts_with("serialized_") {
        ThreadLevel::Serialized
    } else {
        ThreadLevel::Funneled
    };
    // The per-operation deadline (`AKANTU_MPI_TEST_TIMEOUT_S`, default 20 s,
    // as the other acceptance binaries): the memcheck tier scales it for
    // delayed_sender_completes under valgrind (ci/memcheck/run-in-image.sh).
    let c = world(level, Some(op_timeout()), SendMode::Eager);
    let r = if c.size() == 1 && TWO_RANKS.contains(&name.as_str()) {
        println!("INFO {name}: not applicable on one rank");
        Ok(())
    } else {
        // Start the choreographies together.
        c.barrier()
            .map_err(|e| e.to_string())
            .and_then(|()| match name.as_str() {
                "delayed_sender_completes" => delayed_sender_completes(&c),
                "serialized_progress" => serialized_progress(&c),
                "serialized_crossed_dup" => serialized_crossed_dup(&c),
                "serialized_crossed_split" => serialized_crossed_split(&c),
                "serialized_crossed_native" => serialized_crossed_native(&c),
                "serialized_crossed_mixed" => serialized_crossed_mixed(&c),
                "serialized_exclusive_rule" => serialized_exclusive_rule(&c),
                "serialized_split_vs_dup_low" => serialized_split_vs_dup_low(&c),
                "serialized_split_vs_dup_high" => serialized_split_vs_dup_high(&c),
                "serialized_split_vs_world" => serialized_split_vs_world(&c),
                "serialized_creation_rule" => serialized_creation_rule(&c),
                "serialized_self_wait_progress" => serialized_self_wait_progress(&c),
                "idle_wait_cpu" => idle_wait_cpu(&c),
                _ => unreachable!(),
            })
    };
    match &r {
        Ok(()) => println!("PROGRESS PASS {name} rank={}", c.rank()),
        Err(e) => println!("PROGRESS FAIL {name} rank={}: {e}", c.rank()),
    }
    let code = i32::from(r.is_err());
    drop(c); // last guard on the main thread: MPI_Finalize
    std::process::exit(code);
}
