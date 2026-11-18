// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/test_communicator.cc
// (Bcast, ReceiveAny), test_synchronizer_communication.cc (generic part:
// SynchroneOnce, Synchrone, Asynchrone with the TestAccessor of
// test_data_accessor.hh), test_dof_synchronizer.cc (gather / scatter /
// synchronize of arrays, on hand-built schemes); plus the akantu-rs
// collective-protocol acceptance scenarios (`docs/ARCHITECTURE.md`, INT-009).

//! Real-MPI acceptance tests (INT-009), `harness = false`:
//! launched by `scripts/mpi/run-tests.sh` under `mpirun -np 1/2/4` (8 in the
//! expensive tier), or as an MPI singleton by `cargo test --features mpi`.
//! Every scenario runs on every rank in the same order; a control
//! communicator agrees on each outcome; every wait has a deadline and the
//! process has a watchdog, so nothing hangs.

#[macro_use]
mod mpi_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::{InitOwnership, ThreadLevel};
use akantu_comm::phase::{CollectiveChecks, Phases, Severity, agree, run_phase};
use akantu_comm::synchronizer::{SanityCheck, ScatterGatherEntities, SendRecv};
use akantu_comm::{
    CommunicationBuffer, Communicator, DataAccessor, MpiOptions, ReduceOp, ReductionPolicy,
    SendMode, SendRequest, SimpleIdxDataAccessor, Synchronizer, SynchronizerRegistry, Tag,
    ValueLoc, mpi_lifecycle, run_local,
};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Element, ElementType, Error, FatalKind, GhostType, Idx, Result};
use mpi_common::{Harness, Outcome, op_timeout, start_watchdog, world, world_with_policy};

const LEVEL: ThreadLevel = ThreadLevel::Multiple;

fn fresh() -> Communicator {
    world(LEVEL, Some(op_timeout()), SendMode::Eager)
}

fn e2s(e: Error) -> String {
    e.to_string()
}

/// Bounded time for "no rank waited for a deadline" assertions.
const FAST: Duration = Duration::from_secs(3);

// ------------------------------------------------------- upstream ports

/// `TEST(Communicator, Bcast)`.
fn upstream_bcast() -> Outcome {
    let c = fresh();
    let mut r = [if c.rank() == 0 { 0xdead_beef_u32 } else { 0 }];
    c.broadcast(&mut r, 0).map_err(e2s)?;
    ensure!(r[0] == 0xdead_beef, "got {:#x}", r[0]);
    Ok(())
}

/// Deterministic generator (fixed seed, D-035) replacing the test's
/// `std::random_device` + `mt19937` (only the draws' range matters).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, lo: i32, hi: i32) -> i32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let span = u64::try_from(hi - lo + 1).unwrap_or(1);
        lo + i32::try_from((self.0 >> 33) % span).unwrap_or(0)
    }
}

/// `TEST(Communicator, ReceiveAny)` with the Rust contract (UD-076): one
/// tag per round. Returns the number of anomalies (0 expected).
fn receive_any_rounds(c: &Communicator, rounds: i32, tag_per_round: bool) -> Result<usize> {
    let (rank, size) = (c.rank(), c.size());
    let mut rng = Lcg(0x5eed);
    let mut anomalies = 0usize;
    for n in 0..rounds {
        let tag = Tag(if tag_per_round { 2000 + n } else { 2000 });
        let mut sends = vec![0i32; size.saturating_sub(1)];
        if rank == 0 {
            for s in &mut sends {
                *s = rng.next(1, 10);
            }
        }
        c.broadcast(&mut sends, 0)?;
        if rank == 0 {
            let mut reqs: Vec<SendRequest<i32>> = Vec::new();
            let mut nb_recvs = 0i32;
            for (i, &send) in sends.iter().enumerate() {
                let p = i32::try_from(i).unwrap_or(0) + 1;
                if send > 5 {
                    reqs.push(c.isend_mode(vec![send], p, tag, SendMode::Rendezvous)?);
                }
                if p <= send {
                    nb_recvs += 1;
                }
            }
            c.receive_any_number(reqs, tag, |proc, msg| {
                let k = usize::try_from(proc - 1).unwrap_or(0);
                if msg.first() != Some(&(sends[k] + 100 * proc)) || proc > sends[k] {
                    anomalies += 1;
                }
                nb_recvs -= 1;
                Ok(())
            })?;
            if nb_recvs != 0 {
                anomalies += 1;
            }
        } else {
            let k = usize::try_from(rank - 1).unwrap_or(0);
            let mut reqs = Vec::new();
            if rank <= sends[k] {
                reqs.push(c.isend_mode(
                    vec![sends[k] + 100 * rank],
                    0,
                    tag,
                    SendMode::Rendezvous,
                )?);
            }
            let mut has_recv = false;
            c.receive_any_number(reqs, tag, |proc, msg| {
                if msg.first() != Some(&sends[k]) || proc != 0 {
                    anomalies += 1;
                }
                has_recv = true;
                Ok(())
            })?;
            if has_recv != (sends[k] > 5) {
                anomalies += 1;
            }
        }
    }
    Ok(anomalies)
}

fn upstream_receive_any() -> Outcome {
    let c = fresh();
    let a = receive_any_rounds(&c, 100, true).map_err(e2s)?;
    ensure!(a == 0, "{a} anomalies with per-round tags");
    Ok(())
}

/// Diagnostic for UD-076: the upstream test's single tag across rounds.
/// Anomalies are reported, not asserted (they are timing dependent).
fn receive_any_same_tag_diagnostic() -> Outcome {
    let c = fresh();
    let a = receive_any_rounds(&c, 100, false).map_err(e2s)?;
    let mut total = [u64::try_from(a).unwrap_or(0)];
    c.all_reduce(&mut total, ReduceOp::Sum).map_err(e2s)?;
    if c.rank() == 0 {
        println!(
            "DIAG receive_any_same_tag: {} anomalies over 100 rounds on {} ranks",
            total[0],
            c.size()
        );
    }
    Ok(())
}

// -------------------------------------- serial ≡ local world ≡ MPI-n

/// Per-rank contributions whose sum depends on the association order:
/// rank order gives `((1e16 + 1) - 1e16) + 1 = 1`, a pairwise tree
/// `(1e16 + 1) + (-1e16 + 1) = 0` (`1e16 ± 1` rounds to even).
const ORDER_SENSITIVE: [f64; 4] = [1e16, 1.0, -1e16, 1.0];

/// A deterministic workload over every collective; returns the bit
/// patterns of every result on this rank.
fn workload(c: &Communicator) -> Result<Vec<u64>> {
    let r = c.rank();
    let p = i32::try_from(c.size()).unwrap_or(1);
    let rf = f64::from(r);
    let mut out = Vec::new();
    let bits = |v: &[f64], out: &mut Vec<u64>| out.extend(v.iter().map(|x| x.to_bits()));
    // Cancellation: the sum depends on the reduction order for p >= 3.
    let mut s = [
        ORDER_SENSITIVE[usize::try_from(r).unwrap_or(0) % 4],
        rf * 0.3 + 0.1 + (rf + 1.0).sqrt() * 1e-3,
        1.0 / (rf + 3.0),
    ];
    c.all_reduce(&mut s, ReduceOp::Sum)?;
    bits(&s, &mut out);
    let mut pr = [1.0 + rf * 1e-8, 0.5f64.powi(r)];
    c.all_reduce(&mut pr, ReduceOp::Prod)?;
    bits(&pr, &mut out);
    // Signed zeros and NaN: order-sensitive min/max conventions.
    for op in [ReduceOp::Min, ReduceOp::Max] {
        let mut m = [
            if r % 2 == 0 { 0.0 } else { -0.0 },
            if r == 1 { f64::NAN } else { rf },
            -rf,
        ];
        c.all_reduce(&mut m, op)?;
        bits(&m, &mut out);
    }
    let mut f = [0.1f32 * (r as f32 + 1.0)];
    c.all_reduce(&mut f, ReduceOp::Sum)?;
    out.push(u64::from(f[0].to_bits()));
    // Integers: wrapping, bit and logical operations.
    let mut i = [(i32::MAX - 1).wrapping_add(r), r * 7, -r];
    c.all_reduce(&mut i, ReduceOp::Sum)?;
    out.extend(i.iter().map(|&x| u64::from(x.cast_unsigned())));
    for op in [
        ReduceOp::BitAnd,
        ReduceOp::BitOr,
        ReduceOp::BitXor,
        ReduceOp::LogicalAnd,
        ReduceOp::LogicalOr,
        ReduceOp::LogicalXor,
        ReduceOp::Min,
        ReduceOp::Max,
        ReduceOp::Prod,
    ] {
        let mut u = [
            0xF0F0u64 ^ (u64::try_from(r).unwrap_or(0) * 3),
            u64::from(r % 2 == 0),
        ];
        c.all_reduce(&mut u, op)?;
        out.extend_from_slice(&u);
        let mut b = [u8::try_from(r % 256).unwrap_or(0) | 1, 0x80];
        c.all_reduce(&mut b, op)?;
        out.extend(b.iter().map(|&x| u64::from(x)));
        let mut sb = [i8::try_from(r % 100).unwrap_or(0) - 3];
        c.all_reduce(&mut sb, op)?;
        out.push(u64::from(sb[0].cast_unsigned()));
    }
    // Scans.
    let mut sc = [rf * 0.1 + 1e15 * if r % 2 == 0 { 1.0 } else { -1.0 }, rf];
    c.scan(&mut sc, ReduceOp::Sum)?;
    bits(&sc, &mut out);
    let mut ex = [rf * 0.7 + 0.01];
    c.exclusive_scan(&mut ex, ReduceOp::Sum)?;
    bits(&ex, &mut out);
    out.push(c.exclusive_scan_sum_u64(u64::try_from(r).unwrap_or(0) + 5)?);
    let mut exm = [r + 10];
    c.exclusive_scan(&mut exm, ReduceOp::Max)?;
    out.push(u64::from(exm[0].cast_unsigned()));
    // Broadcast (from the last rank) and dynamic broadcast.
    let mut b = [rf.sin(), rf.cos()];
    c.broadcast(&mut b, p - 1)?;
    bits(&b, &mut out);
    let v = c.broadcast_vec(&vec![r; usize::try_from(r + 2).unwrap_or(0)], p - 1)?;
    out.extend(v.iter().map(|&x| u64::from(x.cast_unsigned())));
    // Gathers.
    let g = c.all_gather_v(&vec![rf * 2.5; usize::try_from(r % 3).unwrap_or(0)])?;
    for part in &g {
        out.push(part.len() as u64);
        bits(part, &mut out);
    }
    out.extend(
        c.all_gather(&[r * 3, -r])?
            .iter()
            .map(|&x| u64::from(x.cast_unsigned())),
    );
    match c.gather_v(
        &vec![u64::try_from(r).unwrap_or(0); usize::try_from(r).unwrap_or(0) + 1],
        p - 1,
    )? {
        Some(parts) => {
            for part in parts {
                out.push(part.len() as u64);
                out.extend(part);
            }
        }
        None => out.push(u64::MAX),
    }
    if let Some(gg) = c.gather(&[rf + 0.5], 0)? {
        bits(&gg, &mut out);
    }
    // Scatters from rank 0.
    let blocks: Vec<Vec<f64>> = (0..p)
        .map(|k| vec![f64::from(k) * 1.5; usize::try_from(k).unwrap_or(0) + 1])
        .collect();
    let mine = c.scatter_v((r == 0).then_some(blocks.as_slice()), 0)?;
    bits(&mine, &mut out);
    let flat: Vec<i64> = (0..i64::from(p) * 2).collect();
    let part = c.scatter((r == 0).then_some(flat.as_slice()), 0)?;
    out.extend(part.iter().map(|&x| x.cast_unsigned()));
    // Rooted reduction (non-root unchanged).
    let mut red = [rf * 0.25 + 1e-17, 1.0 - rf];
    c.reduce(&mut red, ReduceOp::Sum, (1 % p).min(p - 1))?;
    bits(&red, &mut out);
    // Min/max-loc with ties.
    let mut loc = [
        ValueLoc {
            value: f64::from(r % 2),
            loc: 100 - r,
        },
        ValueLoc { value: -rf, loc: r },
    ];
    c.all_reduce_loc(&mut loc, ReduceOp::MinLoc)?;
    for l in loc {
        out.push(l.value.to_bits());
        out.push(u64::from(l.loc.cast_unsigned()));
    }
    let mut mx = [ValueLoc {
        value: f64::from(r / 2),
        loc: r,
    }];
    c.all_reduce_loc(&mut mx, ReduceOp::MaxLoc)?;
    out.push(mx[0].value.to_bits());
    out.push(u64::from(mx[0].loc.cast_unsigned()));
    c.barrier()?;
    Ok(out)
}

fn serial_local_mpi_equivalence() -> Outcome {
    let c = fresh();
    let mpi = workload(&c).map_err(e2s)?;
    let size = c.size();
    let local = run_local(size, Duration::from_secs(30), |lc| {
        workload(&lc).map_err(e2s)
    });
    let mine = local[usize::try_from(c.rank()).unwrap_or(0)].clone()?;
    ensure!(
        mpi == mine,
        "MPI and local-world results differ at {:?}",
        mpi.iter().zip(&mine).position(|(a, b)| a != b)
    );
    if size == 1 {
        let serial = workload(&Communicator::serial()).map_err(e2s)?;
        ensure!(serial == mpi, "serial and MPI-1 results differ");
    }
    Ok(())
}

/// Evidence for NUMERICAL_VALIDATION: OpenMPI's native `MPI_Allreduce`
/// (rsmpi, implementation order) vs the rank-ordered fold on data whose sum
/// depends on the order. Reported, not asserted.
fn native_allreduce_order_diagnostic() -> Outcome {
    use mpi::collective::SystemOperation;
    use mpi::traits::CommunicatorCollectives;
    let c = fresh();
    let r = f64::from(c.rank());
    let _ = r;
    let x = ORDER_SENSITIVE[usize::try_from(c.rank()).unwrap_or(0) % 4];
    let mut ordered = [x];
    c.all_reduce(&mut ordered, ReduceOp::Sum).map_err(e2s)?;
    let w = mpi::topology::SimpleCommunicator::world();
    let mut native = 0.0f64;
    w.all_reduce_into(&x, &mut native, SystemOperation::sum());
    let same = native.to_bits() == ordered[0].to_bits();
    let mut differ = [u8::from(!same)];
    c.all_reduce(&mut differ, ReduceOp::Max).map_err(e2s)?;
    if c.rank() == 0 {
        println!(
            "DIAG native_allreduce: ranks={} rank-ordered={:e} native={:e} bitwise-equal={} any-rank-differs={}",
            c.size(),
            ordered[0],
            native,
            same,
            differ[0] != 0
        );
    }
    Ok(())
}

// ----------------------------------------------------- communicator API

fn communicator_api() -> Outcome {
    let c = fresh();
    let (r, p) = (c.rank(), c.size());
    let pi = i32::try_from(p).map_err(|e| e.to_string())?;
    let next = (r + 1) % pi;
    let prev = (r + pi - 1) % pi;
    // Ring: non-blocking receive posted first, owned send, completion.
    let rq = c.irecv::<f64>(3, prev, Tag(11)).map_err(e2s)?;
    let sq = c.isend(vec![f64::from(r); 3], next, Tag(11)).map_err(e2s)?;
    ensure!(rq.wait().map_err(e2s)? == vec![f64::from(prev); 3]);
    ensure!(sq.wait().map_err(e2s)?.len() == 3, "buffer returned");
    // Blocking send/receive in a ring of synchronous sends (receives
    // posted through irecv first: no reliance on buffering).
    let rq = c.irecv::<i64>(2, prev, Tag(12)).map_err(e2s)?;
    let sq = c
        .isend_mode(vec![i64::from(r), -1], next, Tag(12), SendMode::Rendezvous)
        .map_err(e2s)?;
    ensure!(rq.wait().map_err(e2s)? == vec![i64::from(prev), -1]);
    sq.wait().map_err(e2s)?;
    // wait_received returns the actual length, at most the posted one
    // (wire contract is the caller's post-check; longer: truncated, see
    // cm05_posted_receives).
    let rq = c.irecv::<u8>(4, prev, Tag(13)).map_err(e2s)?;
    c.send(&[1u8, 2, 3], next, Tag(13)).map_err(e2s)?;
    ensure!(rq.wait_received().map_err(e2s)?.len() == 3);
    // An exact-length wait detects a size mismatch.
    let rq = c.irecv::<u8>(1, prev, Tag(14)).map_err(e2s)?;
    c.send(&[1u8, 2], next, Tag(14)).map_err(e2s)?;
    ensure!(
        rq.wait().is_err(),
        "exact wait must reject a 2-byte message"
    );
    // Probes: every rank sends to rank 0, which probes any source.
    if r != 0 {
        c.send(&vec![r; usize::try_from(r).unwrap_or(0)], 0, Tag(15))
            .map_err(e2s)?;
    } else {
        let mut seen = Vec::new();
        for _ in 1..p {
            let st = c.probe::<i32>(None, Tag(15)).map_err(e2s)?;
            ensure!(st.count == usize::try_from(st.source).unwrap_or(0) && st.tag == Tag(15));
            let msg = c.receive_vec::<i32>(st.source, Tag(15)).map_err(e2s)?;
            ensure!(msg.iter().all(|&x| x == st.source));
            seen.push(st.source);
        }
        seen.sort_unstable();
        ensure!(seen == (1..pi).collect::<Vec<_>>());
        ensure!(c.iprobe::<i32>(None, Tag(15)).map_err(e2s)?.is_none());
    }
    // Synchronous send test-before-match reports incomplete.
    if p > 1 {
        if r == 0 {
            let mut s = c
                .isend_mode(vec![5u32], 1, Tag(16), SendMode::Rendezvous)
                .map_err(e2s)?;
            let early = s.test().map_err(e2s)?;
            c.barrier().map_err(e2s)?;
            c.barrier().map_err(e2s)?;
            ensure!(!early, "Issend completed before the receive was matched");
            s.wait().map_err(e2s)?;
        } else {
            c.barrier().map_err(e2s)?;
            if r == 1 {
                ensure!(c.receive_vec::<u32>(0, Tag(16)).map_err(e2s)? == vec![5]);
            }
            c.barrier().map_err(e2s)?;
        }
    }
    // Non-blocking barrier.
    let mut b = c.ibarrier().map_err(e2s)?;
    let t0 = Instant::now();
    while !b.test().map_err(e2s)? {
        ensure!(t0.elapsed() < op_timeout(), "ibarrier never completed");
        std::thread::yield_now();
    }
    // Tag outside the portable range is rejected locally.
    ensure!(c.isend(vec![1u8], next, Tag(40_000)).is_err());
    ensure!(c.send(&[1u8], pi, Tag(1)).is_err(), "rank out of range");
    c.barrier().map_err(e2s)?;
    Ok(())
}

/// Checked collectives: mismatched operations, invalid root inputs and
/// inconsistent broadcast lengths fail on **every** rank, promptly, and
/// leave the communicator usable (no pending operation).
fn checked_collectives() -> Outcome {
    let c = fresh();
    let (r, p) = (c.rank(), c.size());
    let t0 = Instant::now();
    if p > 1 {
        let e = if r == 0 {
            c.all_reduce(&mut [1u8], ReduceOp::Sum).err()
        } else {
            c.barrier().err()
        };
        ensure!(
            e.as_ref()
                .is_some_and(|e| e.to_string().contains("mismatch")),
            "{e:?}"
        );
        let len = if r == 0 { 2 } else { 3 };
        let e = c.broadcast(&mut vec![0u32; len], 0).err();
        ensure!(e.is_some(), "length mismatch must fail everywhere");
    }
    // Invalid scatter input on the root: every rank fails.
    let e = c
        .scatter_v::<u8>(if r == 0 { Some(&[]) } else { None }, 0)
        .err();
    ensure!(e.is_some(), "invalid root input must fail everywhere");
    let e = c.scatter::<u8>(if r == 0 { Some(&[1u8; 1]) } else { None }, 0);
    if p > 1 {
        ensure!(e.is_err(), "indivisible scatter must fail everywhere");
    }
    ensure!(
        t0.elapsed() < FAST,
        "checked failures must not wait for a deadline"
    );
    // Still usable.
    let mut v = [1u64];
    c.all_reduce(&mut v, ReduceOp::Sum).map_err(e2s)?;
    ensure!(v[0] == p as u64);
    ensure!(c.broken().is_none());
    Ok(())
}

fn split_and_duplicate() -> Outcome {
    let c = fresh();
    let r = c.rank();
    let sub = c
        .split(Some(u32::try_from(r % 2).unwrap_or(0)), -r)
        .map_err(e2s)?
        .ok_or("split returned no communicator")?;
    let members: Vec<i32> = (0..i32::try_from(c.size()).unwrap_or(1))
        .filter(|k| k % 2 == r % 2)
        .collect();
    ensure!(sub.size() == members.len());
    // Ranks ordered by key = -rank: highest parent rank first.
    let expected_rank = members
        .iter()
        .rev()
        .position(|&k| k == r)
        .unwrap_or(usize::MAX);
    ensure!(
        usize::try_from(sub.rank()).unwrap_or(0) == expected_rank,
        "sub rank {}",
        sub.rank()
    );
    let mut s = [u64::try_from(r).unwrap_or(0)];
    sub.all_reduce(&mut s, ReduceOp::Sum).map_err(e2s)?;
    ensure!(
        s[0] == members
            .iter()
            .map(|&k| u64::try_from(k).unwrap_or(0))
            .sum::<u64>()
    );
    let none = c
        .split(if r == 0 { None } else { Some(7) }, 0)
        .map_err(e2s)?;
    ensure!(none.is_some() == (r != 0));
    // Independent scope registries: a live scope on the parent does not
    // refuse the duplicate's raw operations.
    let dup = c.duplicate().map_err(e2s)?;
    {
        let _owner = Phases::new(&c, "parent-owner");
        ensure!(
            c.barrier().is_err(),
            "parent raw op refused under its scope"
        );
        dup.barrier().map_err(e2s)?;
    }
    // Finalize handler runs on the last handle's drop (MPI-023).
    let hit = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let h = Arc::clone(&hit);
    dup.on_finalize(move || h.store(true, std::sync::atomic::Ordering::SeqCst));
    drop(dup);
    ensure!(hit.load(std::sync::atomic::Ordering::SeqCst));
    c.barrier().map_err(e2s)?;
    Ok(())
}

// ------------------------------------------------- collective protocol

/// Port of `phase.rs::injected_failures_complete_on_all_ranks` on a ring.
fn ring_exchange(c: &Communicator, bad: Option<i32>, fail_between: bool) -> Result<Vec<f64>> {
    let p = i32::try_from(c.size()).unwrap_or(1);
    let (next, prev) = ((c.rank() + 1) % p, (c.rank() + p - 1) % p);
    let n = run_phase(
        c,
        "sizes",
        || {
            if Some(c.rank()) == bad {
                Err(Error::InvalidArgument("local validation failed".into()))
            } else {
                Ok(3usize)
            }
        },
        Ok,
    )?;
    let local_post = if fail_between && c.rank() == p - 1 {
        Err(Error::Numerical("NaN in packed data".into()))
    } else {
        Ok(())
    };
    agree(c, "pack", local_post, Severity::Recoverable)?;
    let rq = c.irecv::<f64>(n, prev, Tag(1))?;
    let sq = c.isend(vec![f64::from(c.rank()); n], next, Tag(1))?;
    let v = rq.wait()?;
    sq.wait()?;
    Ok(v)
}

fn phase_injected_failures() -> Outcome {
    let c = fresh();
    let p = i32::try_from(c.size()).unwrap_or(1);
    let mut cases: Vec<(Option<i32>, bool)> = vec![(None, false), (None, true)];
    cases.extend((0..p).map(|b| (Some(b), false)));
    for (bad, between) in cases {
        let t0 = Instant::now();
        let out = ring_exchange(&c, bad, between);
        ensure!(
            t0.elapsed() < FAST,
            "case {bad:?}/{between}: waited for a deadline"
        );
        let ok = bad.is_none() && !between;
        ensure!(out.is_ok() == ok, "case {bad:?}/{between}: {out:?}");
        if bad == Some(c.rank()) {
            ensure!(
                matches!(out, Err(Error::InvalidArgument(_))),
                "failing rank keeps its error"
            );
        }
    }
    ensure!(c.broken().is_none());
    Ok(())
}

/// A four-step operation (prepare agreement, collective, exchange, commit
/// agreement) with a failure injected on rank `f` at step `j`: every rank
/// fails at the same step, with the same agreed severity, without waiting
/// for a deadline; later agreements do not communicate.
fn phase_failure_sweep() -> Outcome {
    let c = fresh();
    let p = i32::try_from(c.size()).unwrap_or(1);
    let steps = 4;
    for f in 0..p {
        for j in 0..steps {
            for fatal in [false, true] {
                let t0 = Instant::now();
                let mut ph = Phases::new(&c, format!("sweep-{f}-{j}"));
                let inject = |k: usize| -> Result<()> {
                    if c.rank() == f && k == j {
                        let e = Error::invalid(format!("injected at step {k}"));
                        Err(if fatal {
                            Error::fatal(FatalKind::Agreed, e)
                        } else {
                            e
                        })
                    } else {
                        Ok(())
                    }
                };
                let r = (|| -> Result<()> {
                    ph.agree("prepare", inject(0), Severity::Recoverable)?;
                    ph.run_collective("count", inject(1).map(|()| 1u64), |cm, v| {
                        let mut x = [v];
                        cm.all_reduce(&mut x, ReduceOp::Sum)?;
                        Ok(x[0])
                    })?;
                    let (next, prev) = ((c.rank() + 1) % p, (c.rank() + p - 1) % p);
                    ph.run("exchange", inject(2), |cm, ()| {
                        let rq = cm.irecv::<u32>(1, prev, Tag(3))?;
                        let sq = cm.isend(vec![7u32], next, Tag(3))?;
                        let v = rq.wait()?;
                        sq.wait()?;
                        Ok(if v == vec![7] {
                            Ok(())
                        } else {
                            Err(Error::invalid("bad payload"))
                        })
                    })?;
                    ph.agree("commit", inject(3), Severity::Recoverable)
                })();
                let settled = ph.settle(r);
                ensure!(t0.elapsed() < FAST, "f={f} j={j}: waited for a deadline");
                ensure!(settled.is_err(), "f={f} j={j}: every rank fails");
                let kind = settled.as_ref().err().and_then(Error::fatal_kind);
                ensure!(
                    kind == fatal.then_some(FatalKind::Agreed),
                    "f={f} j={j} fatal={fatal}: kind {kind:?}"
                );
                ensure!(ph.is_failed() && !ph.is_broken());
                ensure!(ph.agree("later", Ok(()), Severity::Recoverable).is_err());
                // Identical agreed severity everywhere.
                let sev = [ph.failure().map_or(0u8, |s| s as u8)];
                drop(ph);
                let all = c.all_gather(&sev).map_err(e2s)?;
                ensure!(all.iter().all(|&s| s == sev[0]), "severities {all:?}");
            }
        }
    }
    ensure!(c.broken().is_none());
    Ok(())
}

/// Port of `phase.rs::{failure_in_different_code_paths_is_bounded,
/// digest_and_post_failures_are_agreed, agreed_severity_is_common_knowledge,
/// nested_independent_scopes_and_raw_operations_are_refused,
/// typed_fatal_local_results_are_agreed_fatal,
/// free_agreements_classify_fatal_outcomes}` on MPI.
fn phase_protocol_ports() -> Outcome {
    let c = fresh();
    let p = c.size();
    let t0 = Instant::now();
    // Different code paths.
    {
        let mut ph = Phases::new(&c, "op");
        let consumer = if c.rank() == 0 {
            Err(Error::Mesh("re-entrant mutation refused locally".into()))
        } else {
            ph.run("consumer:exchange", Ok(()), |comm, ()| {
                let mut v = [1u64];
                comm.all_reduce(&mut v, ReduceOp::Sum)?;
                Ok(Ok(v[0]))
            })
            .map(|_| ())
        };
        let post = if ph.is_failed() {
            consumer
        } else {
            ph.agree("owner:post", consumer, Severity::Fatal)
        };
        ensure!(post.is_err() && ph.is_failed());
        ensure!(ph.agree("later", Ok(()), Severity::Fatal).is_err());
    }
    // Digest and post failures.
    {
        let mut ph = Phases::new(&c, "digest");
        let d = ph.agree_digest(
            "schedule",
            Ok(()),
            u64::from(c.rank() == 0),
            Severity::Recoverable,
        );
        ensure!(p == 1 || (d.is_err() && ph.failure() == Some(Severity::Recoverable)));
    }
    {
        // D-018's post agreement (every-phase checks): the failure is agreed
        // at once on every rank
        let mode = c.collective_checks();
        c.set_collective_checks(CollectiveChecks::EveryPhase);
        let mut ph = Phases::new(&c, "post");
        let post = ph.run("unpack", Ok(()), |comm, ()| {
            comm.barrier()?;
            Ok(if comm.rank() == 0 {
                Err(Error::Numerical("bad received value".into()))
            } else {
                Ok(())
            })
        });
        ensure!(post.is_err() && ph.is_failed() && !ph.is_broken());
        drop(ph);
        // D-126 commit points: rank 0's post-processing failure is deferred
        // (only rank 0 has an error) and agreed on every rank when the scope
        // settles
        c.set_collective_checks(CollectiveChecks::CommitPoints);
        let mut ph = Phases::new(&c, "post-commit-points");
        let post = ph.run("unpack", Ok(()), |comm, ()| {
            comm.barrier()?;
            Ok(if comm.rank() == 0 {
                Err(Error::Numerical("bad received value".into()))
            } else {
                Ok(())
            })
        });
        ensure!(post.is_err() == (c.rank() == 0) && !ph.is_failed());
        let settled = ph.settle(post);
        ensure!(p == 1 || (settled.is_err() && ph.is_failed() && !ph.is_broken()));
        drop(ph);
        c.set_collective_checks(mode);
    }
    // Agreed severity.
    {
        let mut ph = Phases::new(&c, "fatal");
        let local = if c.rank() == i32::try_from(p - 1).unwrap_or(0) {
            Err(Error::Numerical("irreversible step failed".into()))
        } else {
            Ok(())
        };
        ensure!(ph.agree("commit", local, Severity::Fatal).is_err() && ph.is_fatal());
    }
    // Nested scopes and raw operations refused, reported by the owner.
    {
        let mut owner = Phases::new(&c, "owner");
        let callback = if c.rank() == 0 {
            let mut nested = Phases::new(&c, "callback-sync");
            ensure!(nested.is_refused());
            ensure!(nested.agree("x", Ok(()), Severity::Recoverable).is_err());
            let raw = c.all_reduce(&mut [1u64], ReduceOp::Sum).unwrap_err();
            ensure!(raw.to_string().contains("raw all_reduce refused"), "{raw}");
            Err(raw)
        } else {
            owner.run("callback:exchange", Ok(()), |comm, ()| {
                let mut v = [1u64];
                comm.all_reduce(&mut v, ReduceOp::Sum)?;
                Ok(Ok(()))
            })
        };
        let r = owner.agree("callback", callback, Severity::Recoverable);
        ensure!(r.is_err() && owner.agree("later", Ok(()), Severity::Recoverable).is_err());
    }
    let mut v = [1u64];
    c.all_reduce(&mut v, ReduceOp::Sum).map_err(e2s)?;
    ensure!(v[0] == p as u64, "usable after the owner is gone");
    // Typed fatal results are agreed fatal; plain ones stay recoverable.
    for typed in [true, false] {
        let mut ph = Phases::new(&c, "typed");
        let local = if c.rank() == 0 {
            let e = Error::invalid("callback failure");
            Err(if typed {
                Error::fatal(FatalKind::Agreed, e)
            } else {
                e
            })
        } else {
            Ok(())
        };
        let r = ph.agree("callback", local, Severity::Recoverable);
        let k = typed.then_some(FatalKind::Agreed);
        ensure!(ph.fatal_kind() == k && ph.settle(r).unwrap_err().fatal_kind() == k);
    }
    {
        let local = if c.rank() == 0 {
            Err(Error::fatal(FatalKind::Agreed, Error::invalid("x")))
        } else {
            Ok(())
        };
        let a = agree(&c, "free-typed", local, Severity::Recoverable).unwrap_err();
        ensure!(a.fatal_kind() == Some(FatalKind::Agreed));
    }
    ensure!(t0.elapsed() < FAST, "protocol ports waited for a deadline");
    ensure!(c.broken().is_none());
    Ok(())
}

/// Scope ownership on real MPI (thread level MULTIPLE): a worker without the scope
/// is refused; the explicit transfer keeps one owner, its failure state and
/// its step sequence.
fn park_resume_on_workers() -> Outcome {
    let c = fresh();
    let t0 = Instant::now();
    // Worker without the scope: refused on every thread.
    {
        let mut outer = Phases::new(&c, "outer");
        let local = std::thread::scope(|s| {
            s.spawn(|| {
                if c.barrier().is_ok() {
                    return Err(Error::invalid("worker raw op was not refused"));
                }
                if c.rank() == 0 {
                    Err(Error::invalid("local callback failure"))
                } else {
                    let mut nested = Phases::new(&c, "independent-child");
                    nested.agree("child", Ok(()), Severity::Recoverable)
                }
            })
            .join()
            .unwrap_or_else(|_| Err(Error::invalid("worker panicked")))
        });
        let r = outer.agree("callback:done", local, Severity::Recoverable);
        ensure!(c.size() == 1 || r.is_err());
        ensure!(r.is_err() == outer.is_failed());
    }
    // Explicit transfer with a failure on the worker.
    {
        let parked = Phases::new(&c, "outer2").park();
        ensure!(c.barrier().unwrap_err().to_string().contains("parked"));
        let (parked, failed) = std::thread::scope(|s| {
            s.spawn(|| {
                let mut outer = parked.resume();
                let local = if c.rank() == 0 {
                    Err(Error::invalid("worker failure"))
                } else {
                    Ok(())
                };
                let r = outer.agree("callback:done", local, Severity::Recoverable);
                (outer.park(), r.is_err())
            })
            .join()
            .expect("worker")
        });
        ensure!(failed && parked.is_failed());
        let mut outer = parked.resume();
        ensure!(outer.agree("later", Ok(()), Severity::Recoverable).is_err());
    }
    // Successful transfer continues the step sequence.
    {
        let mut ph = Phases::new(&c, "seq");
        ph.agree("before", Ok(()), Severity::Recoverable)
            .map_err(e2s)?;
        let parked = ph.park();
        let (parked, sum) = std::thread::scope(|s| {
            s.spawn(|| {
                let mut ph = parked.resume();
                let sum = ph.run("exchange", Ok(()), |comm, ()| {
                    let mut v = [u64::try_from(comm.rank()).unwrap_or(0) + 1];
                    comm.all_reduce(&mut v, ReduceOp::Sum)?;
                    Ok(Ok(v[0]))
                });
                (ph.park(), sum)
            })
            .join()
            .expect("worker")
        });
        let n = c.size() as u64;
        ensure!(sum.map_err(e2s)? == n * (n + 1) / 2);
        let mut ph = parked.resume();
        ph.agree("after", Ok(()), Severity::Recoverable)
            .map_err(e2s)?;
        ensure!(ph.agreements() == 4);
    }
    ensure!(t0.elapsed() < FAST);
    ensure!(c.broken().is_none());
    Ok(())
}

// ----------------------------------------------------- synchronizers

/// Hand-built element distribution: global element `g` owned by rank
/// `g % p`; each rank holds ghosts of the elements of its two ring
/// neighbours with `g / p` even.
struct Distribution {
    p: i32,
    r: i32,
    n_global: i32,
}

impl Distribution {
    fn new(c: &Communicator) -> Self {
        let p = i32::try_from(c.size()).unwrap_or(1);
        Self {
            p,
            r: c.rank(),
            n_global: 12 * p,
        }
    }
    fn owner(&self, g: i32) -> i32 {
        g % self.p
    }
    fn owned(&self) -> Vec<i32> {
        (0..self.n_global)
            .filter(|&g| self.owner(g) == self.r)
            .collect()
    }
    fn neighbours(&self) -> Vec<i32> {
        let mut n = vec![(self.r + 1) % self.p, (self.r + self.p - 1) % self.p];
        n.sort_unstable();
        n.dedup();
        n.retain(|&q| q != self.r);
        n
    }
    /// Ghost elements of rank `q` (owned by its neighbours).
    fn ghosts_of(&self, q: i32) -> Vec<i32> {
        let mut nb = vec![(q + 1) % self.p, (q + self.p - 1) % self.p];
        nb.sort_unstable();
        nb.dedup();
        nb.retain(|&o| o != q);
        (0..self.n_global)
            .filter(|&g| nb.contains(&self.owner(g)) && (g / self.p) % 2 == 0)
            .collect()
    }
    fn ghosts(&self) -> Vec<i32> {
        self.ghosts_of(self.r)
    }
    fn local_element(&self, g: i32) -> Element {
        if self.owner(g) == self.r {
            let idx = self.owned().iter().position(|&x| x == g).unwrap_or(0);
            Element::new(
                ElementType::Tetrahedron4,
                i32::try_from(idx).unwrap_or(0),
                GhostType::NotGhost,
            )
        } else {
            let idx = self.ghosts().iter().position(|&x| x == g).unwrap_or(0);
            Element::new(
                ElementType::Tetrahedron4,
                i32::try_from(idx).unwrap_or(0),
                GhostType::Ghost,
            )
        }
    }
    fn synchronizer(&self, c: &Communicator, id: &str) -> Synchronizer<Element> {
        let mut s = Synchronizer::new(id, c.clone());
        for q in self.neighbours() {
            let send: Vec<Element> = self
                .ghosts_of(q)
                .into_iter()
                .filter(|&g| self.owner(g) == self.r)
                .map(|g| self.local_element(g))
                .collect();
            let recv: Vec<Element> = self
                .ghosts()
                .into_iter()
                .filter(|&g| self.owner(g) == q)
                .map(|g| self.local_element(g))
                .collect();
            s.communications.send.insert(q, send);
            s.communications.recv.insert(q, recv);
        }
        s
    }
}

fn barycenter(g: i32) -> [f64; 3] {
    let x = f64::from(g);
    [x * 0.1 + 1.0 / 3.0, (x * 0.37).sin(), 1.0 / (x + 7.0)]
}

/// The upstream `TestAccessor`: packs barycenters, the receiver checks
/// them (here bitwise; upstream `EXPECT_NEAR(0, dist, 1e-15)`).
struct TestAccessor {
    owned: Vec<[f64; 3]>,
    ghosts_expected: Vec<[f64; 3]>,
    checked: usize,
    mismatches: usize,
}

impl TestAccessor {
    fn new(d: &Distribution) -> Self {
        Self {
            owned: d.owned().into_iter().map(barycenter).collect(),
            ghosts_expected: d.ghosts().into_iter().map(barycenter).collect(),
            checked: 0,
            mismatches: 0,
        }
    }
    fn get(&self, e: &Element) -> Result<[f64; 3]> {
        let i = usize::try_from(e.index()).map_err(|_| Error::invalid("bad index"))?;
        let v = if e.ghost_type() == GhostType::Ghost {
            &self.ghosts_expected
        } else {
            &self.owned
        };
        v.get(i)
            .copied()
            .ok_or_else(|| Error::invalid("element out of range"))
    }
}

impl DataAccessor<Element> for TestAccessor {
    fn nb_data(&self, e: &[Element], _: SynchronizationTag) -> usize {
        3 * 8 * e.len()
    }
    fn pack_data(
        &self,
        b: &mut CommunicationBuffer,
        e: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        for el in e {
            b.pack_slice(&self.get(el)?);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut CommunicationBuffer,
        e: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        for el in e {
            let mut got = [0.0f64; 3];
            b.unpack_slice(&mut got)?;
            let exp = self.get(el)?;
            self.checked += 1;
            if got.iter().zip(exp).any(|(a, b)| a.to_bits() != b.to_bits()) {
                self.mismatches += 1;
            }
        }
        Ok(())
    }
}

/// `TEST_F(TestElementSynchronizerFixture, SynchroneOnce / Synchrone /
/// Asynchrone)` on a hand-built distribution.
fn upstream_synchronizer_communication() -> Outcome {
    let c = fresh();
    let d = Distribution::new(&c);
    let s = d.synchronizer(&c, "elements");
    let expected_recv: usize = s.communications.recv.values().map(Vec::len).sum();
    for mode in ["once", "synchrone", "asynchrone"] {
        let mut acc = TestAccessor::new(&d);
        match mode {
            "asynchrone" => {
                s.asynchronous_synchronize(&acc, SynchronizationTag::Test)
                    .map_err(e2s)?;
                ensure!(s.has_pending(SynchronizationTag::Test));
                // The communicator is reserved for the pending exchange.
                ensure!(c.barrier().unwrap_err().to_string().contains("parked"));
                ensure!(
                    s.asynchronous_synchronize(&acc, SynchronizationTag::Test)
                        .is_err()
                );
                let delay = 10 + (u64::try_from(c.rank()).unwrap_or(0) * 37) % 91;
                std::thread::sleep(Duration::from_micros(delay));
                s.wait_end_synchronize(&mut acc, SynchronizationTag::Test)
                    .map_err(e2s)?;
                ensure!(!s.has_pending(SynchronizationTag::Test));
            }
            _ => s
                .synchronize(&mut acc, SynchronizationTag::Test)
                .map_err(e2s)?,
        }
        ensure!(
            acc.checked == expected_recv && acc.mismatches == 0,
            "{mode}: {} checked, {} mismatches",
            acc.checked,
            acc.mismatches
        );
    }
    // Registry.
    let mut reg = SynchronizerRegistry::new();
    reg.register_synchronizer(
        Arc::new(d.synchronizer(&c, "elements")),
        SynchronizationTag::Test,
    );
    let mut acc = TestAccessor::new(&d);
    reg.synchronize(SynchronizationTag::Test, &mut acc)
        .map_err(e2s)?;
    reg.synchronize(SynchronizationTag::SmmUv, &mut acc)
        .map_err(e2s)?; // nothing registered
    ensure!(acc.checked == expected_recv && acc.mismatches == 0);
    // Sanity checks on: same result; on one rank only: agreed failure.
    let mut checked = d.synchronizer(&c, "elements");
    checked.set_sanity_checks(true);
    let mut acc = TestAccessor::new(&d);
    checked
        .synchronize(&mut acc, SynchronizationTag::Test)
        .map_err(e2s)?;
    ensure!(acc.mismatches == 0 && acc.checked == expected_recv);
    if c.size() > 1 {
        let mut half = d.synchronizer(&c, "elements");
        half.set_sanity_checks(c.rank() == 0);
        let t0 = Instant::now();
        let r = half.synchronize(&mut TestAccessor::new(&d), SynchronizationTag::Test);
        ensure!(r.is_err() && t0.elapsed() < FAST, "{r:?}");
        ensure!(
            r.unwrap_err().fatal_kind().is_none(),
            "a wire-contract failure stays recoverable"
        );
    }
    ensure!(c.broken().is_none());
    Ok(())
}

/// Ghost → owner reduction (`slave_reduction`) with an additive accessor.
struct SumAccessor {
    owned: Vec<f64>,
    ghosts: Vec<f64>,
}

impl DataAccessor<Element> for SumAccessor {
    fn nb_data(&self, e: &[Element], _: SynchronizationTag) -> usize {
        8 * e.len()
    }
    fn pack_data(
        &self,
        b: &mut CommunicationBuffer,
        e: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        for el in e {
            let i = usize::try_from(el.index()).unwrap_or(0);
            b.pack(if el.ghost_type() == GhostType::Ghost {
                &self.ghosts[i]
            } else {
                &self.owned[i]
            });
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut CommunicationBuffer,
        e: &[Element],
        _: SynchronizationTag,
    ) -> Result<()> {
        for el in e {
            let i = usize::try_from(el.index()).unwrap_or(0);
            let v: f64 = b.unpack()?;
            if el.ghost_type() == GhostType::Ghost {
                self.ghosts[i] += v;
            } else {
                self.owned[i] += v;
            }
        }
        Ok(())
    }
}

fn synchronizer_reduction_and_failures() -> Outcome {
    let c = fresh();
    let d = Distribution::new(&c);
    let s = d.synchronizer(&c, "elements");
    let mut acc = SumAccessor {
        owned: vec![1.0; d.owned().len()],
        ghosts: vec![0.5; d.ghosts().len()],
    };
    s.slave_reduction(&mut acc, SynchronizationTag::SmmRes)
        .map_err(e2s)?;
    // Each owned element receives 0.5 from every rank ghosting it.
    for (k, g) in d.owned().into_iter().enumerate() {
        let copies = (0..d.p)
            .filter(|&q| q != d.r && d.ghosts_of(q).contains(&g))
            .count();
        let exp = 1.0 + 0.5 * copies as f64;
        ensure!(
            acc.owned[k] == exp,
            "element {g}: {} != {exp}",
            acc.owned[k]
        );
    }
    // Rank-specific packing failure on each rank: agreed, prompt.
    struct Failing<'a>(&'a TestAccessor, bool, Option<FatalKind>);
    impl DataAccessor<Element> for Failing<'_> {
        fn nb_data(&self, e: &[Element], t: SynchronizationTag) -> usize {
            self.0.nb_data(e, t)
        }
        fn pack_data(
            &self,
            b: &mut CommunicationBuffer,
            e: &[Element],
            t: SynchronizationTag,
        ) -> Result<()> {
            if self.1 {
                let err = Error::Numerical("injected packing error".into());
                return Err(match self.2 {
                    Some(k) => Error::fatal(k, err),
                    None => err,
                });
            }
            self.0.pack_data(b, e, t)
        }
        fn unpack_data(
            &mut self,
            b: &mut CommunicationBuffer,
            e: &[Element],
            _: SynchronizationTag,
        ) -> Result<()> {
            for _ in e {
                let mut x = [0.0f64; 3];
                b.unpack_slice(&mut x)?;
            }
            Ok(())
        }
    }
    let base = TestAccessor::new(&d);
    for f in 0..d.p {
        for kind in [None, Some(FatalKind::Agreed)] {
            let t0 = Instant::now();
            let r = s.synchronize(
                &mut Failing(&base, c.rank() == f, kind),
                SynchronizationTag::Test,
            );
            if d.p > 1 {
                ensure!(r.is_err(), "f={f}: every rank fails");
                ensure!(r.as_ref().unwrap_err().fatal_kind() == kind, "f={f}: {r:?}");
            }
            ensure!(t0.elapsed() < FAST);
            // Async: failure at the prepare agreement leaves nothing pending.
            let r = s.asynchronous_synchronize(
                &Failing(&base, c.rank() == f, kind),
                SynchronizationTag::Test,
            );
            if d.p == 1 {
                // Nothing to pack on one rank: the exchange is pending.
                r.map_err(e2s)?;
                let mut sink = TestAccessor::new(&d);
                s.wait_end_synchronize(&mut sink, SynchronizationTag::Test)
                    .map_err(e2s)?;
            } else {
                ensure!(r.is_err());
            }
            ensure!(!s.has_pending(SynchronizationTag::Test));
        }
    }
    ensure!(c.broken().is_none());
    Ok(())
}

/// Large bidirectional exchanges with synchronous (rendezvous) sends: no
/// reliance on eager buffering, one-shot and asynchronous.
fn large_rendezvous_exchanges() -> Outcome {
    let c = world(LEVEL, Some(op_timeout()), SendMode::Rendezvous);
    let p = i32::try_from(c.size()).unwrap_or(1);
    for (n, label) in [(131_072usize, "1 MiB"), (2_097_152, "16 MiB")] {
        let mut s: Synchronizer<Idx> = Synchronizer::new(format!("big{n}"), c.clone());
        let ids: Vec<Idx> = (0..i32::try_from(n).unwrap_or(0)).collect();
        for q in [(c.rank() + 1) % p, (c.rank() + p - 1) % p] {
            if q != c.rank() {
                s.communications.send.insert(q, ids.clone());
                s.communications.recv.insert(q, ids.clone());
            }
        }
        let mut data = vec![f64::from(c.rank()); n];
        let t0 = Instant::now();
        s.synchronize_array(&mut data, 1).map_err(e2s)?;
        let neighbours: Vec<i32> = s.communications.recv.keys().copied().collect();
        // Every received value comes from one neighbour (the last unpacked
        // in rank order).
        if let Some(&last) = neighbours.last() {
            ensure!(
                data.iter().all(|&x| x == f64::from(last)),
                "{label}: bad payload"
            );
        }
        let one_shot = t0.elapsed();
        let mut acc =
            akantu_comm::SimpleIdxDataAccessor::new(&mut data, 1, SynchronizationTag::SmmUv);
        s.asynchronous_synchronize(&acc, SynchronizationTag::SmmUv)
            .map_err(e2s)?;
        s.wait_end_synchronize(&mut acc, SynchronizationTag::SmmUv)
            .map_err(e2s)?;
        if c.rank() == 0 {
            println!(
                "INFO {label} bidirectional rendezvous exchange: {:.1} ms",
                one_shot.as_secs_f64() * 1e3
            );
        }
    }
    ensure!(c.broken().is_none());
    Ok(())
}

/// Node-like scheme for root gather/scatter and array synchronization:
/// global node `g` mastered by `g % p`; each rank also holds slave copies
/// of the nodes `g` with `g % 3 == 0` mastered by its successor.
struct Nodes {
    p: i32,
    r: i32,
    n_global: i32,
}

impl Nodes {
    fn new(c: &Communicator) -> Self {
        let p = i32::try_from(c.size()).unwrap_or(1);
        Self {
            p,
            r: c.rank(),
            n_global: 10 * p + 3,
        }
    }
    fn masters(&self, q: i32) -> Vec<i32> {
        (0..self.n_global).filter(|&g| g % self.p == q).collect()
    }
    fn slaves(&self, q: i32) -> Vec<i32> {
        if self.p == 1 {
            return Vec::new();
        }
        let succ = (q + 1) % self.p;
        (0..self.n_global)
            .filter(|&g| g % self.p == succ && g % 3 == 0)
            .collect()
    }
    /// Local numbering: masters then slaves.
    fn local_globals(&self) -> Vec<i32> {
        let mut v = self.masters(self.r);
        v.extend(self.slaves(self.r));
        v
    }
    fn local_of(&self, q: i32, g: i32) -> Idx {
        let mut v = self.masters(q);
        v.extend(self.slaves(q));
        i32::try_from(v.iter().position(|&x| x == g).unwrap_or(0)).unwrap_or(0)
    }
    fn synchronizer(&self, c: &Communicator) -> Synchronizer<Idx> {
        let mut s = Synchronizer::new("nodes", c.clone());
        if self.p == 1 {
            return s;
        }
        let pred = (self.r + self.p - 1) % self.p;
        // Send: my masters that are slaves on my predecessor.
        let send: Vec<Idx> = self
            .slaves(pred)
            .into_iter()
            .map(|g| self.local_of(self.r, g))
            .collect();
        let recv: Vec<Idx> = self
            .slaves(self.r)
            .into_iter()
            .map(|g| self.local_of(self.r, g))
            .collect();
        let succ = (self.r + 1) % self.p;
        if !send.is_empty() {
            s.communications.send.insert(pred, send);
        }
        if !recv.is_empty() {
            s.communications.recv.insert(succ, recv);
        }
        s
    }
}

impl ScatterGatherEntities for Nodes {
    fn owned_entities(&self) -> (Vec<Idx>, Vec<Idx>) {
        let m = self.masters(self.r);
        let local = (0..i32::try_from(m.len()).unwrap_or(0)).collect();
        (local, m)
    }
    fn local_size(&self) -> usize {
        self.local_globals().len()
    }
    fn global_size(&self) -> usize {
        usize::try_from(self.n_global).unwrap_or(0)
    }
}

/// `test_dof_synchronizer.cc` intents on hand-built node schemes: gather
/// on the root by global id, scatter + ghost synchronization, reduction
/// synchronization, scheme filtering.
fn gather_scatter_and_array_sync() -> Outcome {
    let c = fresh();
    let nodes = Nodes::new(&c);
    let mut s = nodes.synchronizer(&c);
    let f = |g: i32| f64::from(g) * 1.25 - 3.0;
    let local_globals = nodes.local_globals();
    // gather: masters' values placed by global id on the root.
    let to_gather: Vec<f64> = local_globals.iter().flat_map(|&g| [f(g), -f(g)]).collect();
    let g = s.gather(&nodes, &to_gather, 2).map_err(e2s)?;
    if c.rank() == 0 {
        let g = g.ok_or("root got nothing")?;
        for k in 0..nodes.n_global {
            let i = usize::try_from(k).unwrap_or(0) * 2;
            ensure!(g[i] == f(k) && g[i + 1] == -f(k), "gathered node {k}");
        }
    } else {
        ensure!(g.is_none());
    }
    // scatter (+ ghost synchronization): every local copy gets f(g).
    let global: Vec<f64> = (0..nodes.n_global)
        .flat_map(|k| [f(k), 2.0 * f(k)])
        .collect();
    let mut scattered = vec![f64::NAN; local_globals.len() * 2];
    s.scatter(
        &nodes,
        &mut scattered,
        (c.rank() == 0).then_some(global.as_slice()),
        2,
    )
    .map_err(e2s)?;
    for (k, &gid) in local_globals.iter().enumerate() {
        ensure!(
            scattered[2 * k] == f(gid) && scattered[2 * k + 1] == 2.0 * f(gid),
            "scattered node {gid}"
        );
    }
    // Cached scheme reused; changed → rebuilt (agreed).
    s.set_entities_changed();
    let g2 = s.gather(&nodes, &to_gather, 2).map_err(e2s)?;
    ensure!(c.rank() != 0 || g2.is_some());
    // Reduction synchronization: masters sum their slave copies, then the
    // slaves get the master value.
    let mut ones = vec![1u64; local_globals.len()];
    s.reduce_synchronize_array(&mut ones, 1, ReduceOp::Sum)
        .map_err(e2s)?;
    for (k, &gid) in local_globals.iter().enumerate() {
        let copies = (0..nodes.p)
            .filter(|&q| nodes.slaves(q).contains(&gid))
            .count() as u64;
        ensure!(
            ones[k] == 1 + copies,
            "node {gid}: {} != {}",
            ones[k],
            1 + copies
        );
    }
    // Invalid root input: agreed failure, prompt, scheme intact.
    let t0 = Instant::now();
    let bad = s.scatter(
        &nodes,
        &mut scattered,
        (c.rank() == 0).then_some(&global[..2]),
        2,
    );
    ensure!(bad.is_err() && t0.elapsed() < FAST);
    // filter_scheme: keep even global ids; synchronization still matches.
    s.filter_scheme(|&l| local_globals[usize::try_from(l).unwrap_or(0)] % 2 == 0)
        .map_err(e2s)?;
    let mut vals: Vec<f64> = local_globals
        .iter()
        .map(|&gid| {
            if gid % nodes.p == nodes.r {
                f(gid)
            } else {
                f64::NAN
            }
        })
        .collect();
    s.synchronize_array(&mut vals, 1).map_err(e2s)?;
    for (k, &gid) in local_globals.iter().enumerate() {
        let is_master = gid % nodes.p == nodes.r;
        let expected_synced = is_master || gid % 2 == 0;
        ensure!(
            expected_synced == !vals[k].is_nan(),
            "node {gid} after filtering"
        );
        if expected_synced {
            ensure!(vals[k] == f(gid));
        }
    }
    // Scheme manipulation API.
    let mut other = nodes.synchronizer(&c);
    let mut moved = Synchronizer::new("moved", c.clone());
    moved.split_from(&mut other, |&l| l % 2 == 0);
    let total = |x: &Synchronizer<Idx>, sr: SendRecv| {
        x.communications
            .schemes(sr)
            .values()
            .map(Vec::len)
            .sum::<usize>()
    };
    let orig = nodes.synchronizer(&c);
    ensure!(
        total(&moved, SendRecv::Send) + total(&other, SendRecv::Send)
            == total(&orig, SendRecv::Send)
    );
    other.swap_send_recv();
    ensure!(
        total(&other, SendRecv::Recv) + total(&moved, SendRecv::Send)
            == total(&orig, SendRecv::Send)
    );
    ensure!(c.broken().is_none());
    Ok(())
}

/// A [`SanityCheck`] hook verifying global ids (mesh-aware synchronizers
/// provide barycenters / node flags the same way).
struct GlobalIdCheck(Vec<i32>);
impl SanityCheck<Idx> for GlobalIdCheck {
    fn data_size(&self, e: &[Idx], _: SynchronizationTag) -> usize {
        4 * e.len()
    }
    fn pack(&self, b: &mut CommunicationBuffer, e: &[Idx], _: SynchronizationTag) -> Result<()> {
        for &l in e {
            b.pack(&self.0[usize::try_from(l).unwrap_or(0)]);
        }
        Ok(())
    }
    fn unpack(
        &self,
        b: &mut CommunicationBuffer,
        e: &[Idx],
        _: SynchronizationTag,
        proc: i32,
        _: i32,
    ) -> Result<()> {
        for &l in e {
            let g: i32 = b.unpack()?;
            if g != self.0[usize::try_from(l).unwrap_or(0)] {
                return Err(Error::Communication(format!(
                    "global id mismatch from {proc}"
                )));
            }
        }
        Ok(())
    }
}

fn sanity_check_hook() -> Outcome {
    let c = fresh();
    let nodes = Nodes::new(&c);
    let mut s = nodes.synchronizer(&c);
    s.set_sanity_checks(true);
    s.set_sanity_check_hook(Arc::new(GlobalIdCheck(nodes.local_globals())));
    let mut v: Vec<f64> = nodes
        .local_globals()
        .iter()
        .map(|&g| f64::from(g))
        .collect();
    s.synchronize_array(&mut v, 1).map_err(e2s)?;
    // A wrong global id on one rank is detected by its receivers.
    let mut wrong = nodes.local_globals();
    if c.rank() == 0 {
        wrong.iter_mut().for_each(|g| *g += 1);
    }
    s.set_sanity_check_hook(Arc::new(GlobalIdCheck(wrong)));
    let r = s.synchronize_array(&mut v, 1);
    ensure!(
        c.size() == 1 || r.is_err(),
        "corrupted check data must fail"
    );
    Ok(())
}

// ------------------------------------- contract regressions

fn opts() -> MpiOptions {
    MpiOptions {
        thread_level: LEVEL,
        timeout: Some(op_timeout()),
        ..MpiOptions::default()
    }
}

/// `MPI_COMM_NULL` and intercommunicators are
/// rejected on every rank before any collective. Before the check a valid
/// 1-versus-(p-1) intercommunicator was accepted and the first header
/// all-gather overran a heap buffer (exit 139). The intracommunicator API
/// (`from_mpi(&SimpleCommunicator)`) works; `from_mpi(&InterCommunicator)`
/// does not compile (doctest).
#[allow(unsafe_code)] // test-only intercommunicator construction (MPI_Intercomm_create)
fn cm01_intercommunicator_import() -> Outcome {
    use mpi::ffi;
    use mpi::raw::AsRaw;
    use mpi::topology::{Color, Communicator as _};
    let c = fresh();
    let (r, p) = (c.rank(), c.size());
    // SAFETY: immutable rsmpi shim constant.
    let null = unsafe { ffi::RSMPI_COMM_NULL };
    // SAFETY: `MPI_COMM_NULL` is allowed by the contract.
    let e = unsafe { Communicator::from_raw_mpi(null, opts()) };
    ensure!(
        matches!(e, Err(Error::InvalidArgument(_))),
        "null handle: {e:?}"
    );
    let w = mpi::topology::SimpleCommunicator::world();
    let d = Communicator::from_mpi(&w, opts()).map_err(e2s)?;
    let mut v = [1u32];
    d.all_reduce(&mut v, ReduceOp::Sum).map_err(e2s)?;
    ensure!(v[0] as usize == p);
    if p < 2 {
        return Ok(());
    }
    // Unequal groups: rank 0 versus ranks 1..p (local sizes 1 and p-1).
    let local = w
        .split_by_color(Color::with_value(i32::from(r != 0)))
        .ok_or("split")?;
    let mut inter = null;
    let remote_leader = if r == 0 { 1 } else { 0 };
    // SAFETY: valid local and peer communicators, valid leaders, a tag,
    // and an out-parameter; freed below.
    let rc = unsafe {
        ffi::MPI_Intercomm_create(
            local.as_raw(),
            0,
            w.as_raw(),
            remote_leader,
            45,
            &raw mut inter,
        )
    };
    ensure!(rc == 0, "MPI_Intercomm_create: {rc}");
    let t0 = Instant::now();
    // SAFETY: `inter` is a valid communicator handle, freed only below.
    let e = unsafe { Communicator::from_raw_mpi(inter, opts()) };
    let rejected = matches!(&e, Err(Error::InvalidArgument(m)) if m.contains("intercommunicator"));
    // SAFETY: the host owns `inter`; akantu did not keep it.
    let rc = unsafe { ffi::MPI_Comm_free(&raw mut inter) };
    ensure!(rejected, "intercommunicator accepted: {e:?}");
    ensure!(rc == 0 && t0.elapsed() < FAST);
    // akantu's communicators are unaffected.
    c.barrier().map_err(e2s)?;
    d.barrier().map_err(e2s)?;
    ensure!(c.broken().is_none());
    Ok(())
}

/// Operation, type and root mismatches and an invalid root,
/// plus count/operator/root-only cases: every rank fails,
/// promptly, with a recoverable error; the communicator stays usable. Before
/// the fix the first two returned `Ok` with wrong data, `root-invalid`
/// returned locally on rank 0 while peers timed out (transport), and
/// `root-mismatch` returned `Ok`.
fn cm02_collective_contract() -> Outcome {
    let c = fresh();
    let (r, p) = (c.rank(), c.size());
    let pi = i32::try_from(p).map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let mut checks: Vec<(&str, Result<()>)> = Vec::new();
    // Must fail on every rank whatever the rank count.
    let mut b = [r];
    checks.push((
        "root-invalid",
        c.broadcast(&mut b, if r == 0 { pi } else { 0 }),
    ));
    checks.push((
        "unsupported-op",
        c.all_reduce(&mut [1.0f64], ReduceOp::BitOr),
    ));
    checks.push(("loc-op-on-int", c.all_reduce(&mut [1i32], ReduceOp::MinLoc)));
    checks.push((
        "reduce-root-invalid",
        c.reduce(&mut [1u8], ReduceOp::Sum, -1),
    ));
    checks.push((
        "scatter-root-input",
        c.scatter_v::<u8>(if r == 0 { Some(&[]) } else { None }, 0)
            .map(|_| ()),
    ));
    if p > 1 {
        let mut x = [r + 1];
        let op = if r == 0 { ReduceOp::Sum } else { ReduceOp::Max };
        checks.push(("op-mismatch", c.all_reduce(&mut x, op)));
        checks.push((
            "type-mismatch",
            if r == 0 {
                c.all_reduce(&mut [1.0f64], ReduceOp::Sum)
            } else {
                c.all_reduce(&mut [1u64], ReduceOp::Sum)
            },
        ));
        let mut b = [r];
        checks.push(("root-mismatch", c.broadcast(&mut b, r)));
        let n = if r == pi - 1 { 3 } else { 2 };
        checks.push((
            "count-allreduce",
            c.all_reduce(&mut vec![1.0f64; n], ReduceOp::Sum),
        ));
        checks.push((
            "count-scan",
            c.exclusive_scan(&mut vec![1u64; n], ReduceOp::Sum),
        ));
        checks.push((
            "count-reduce",
            c.reduce(&mut vec![1i64; n], ReduceOp::Max, 0),
        ));
        checks.push(("count-gather", c.gather(&vec![r; n], 0).map(|_| ())));
        checks.push(("count-all-gather", c.all_gather(&vec![r; n]).map(|_| ())));
        checks.push(("count-broadcast", c.broadcast(&mut vec![0u32; n], 0)));
        checks.push((
            "split-color",
            c.split(Some(if r == 1 { u32::MAX } else { 0 }), 0)
                .map(|_| ()),
        ));
    }
    for (name, res) in &checks {
        match res {
            Ok(()) => return Err(format!("{name}: returned Ok on rank {r}")),
            Err(e) if e.fatal_kind().is_some() => {
                return Err(format!("{name}: fatal on rank {r}: {e}"));
            }
            Err(_) => {}
        }
    }
    ensure!(
        t0.elapsed() < FAST,
        "contract failures must not wait for a deadline"
    );
    // After the `op-mismatch` contract failure, a barrier succeeds.
    c.barrier().map_err(e2s)?;
    let mut v = [1u64];
    c.all_reduce(&mut v, ReduceOp::Sum).map_err(e2s)?;
    ensure!(v[0] == p as u64 && c.broken().is_none());
    Ok(())
}

/// Posted receives are real (matching-engine slots):
/// posting-order matching whatever the
/// wait order, receive progress for a blocking synchronous send ring,
/// cancellation of an unmatched dropped receive, discard of a matched one,
/// and truncation as an ordinary error. Before the fix the first-posted
/// receive got the second message and the synchronous ring timed out.
fn cm05_posted_receives() -> Outcome {
    let c = fresh();
    let (r, p) = (c.rank(), c.size());
    let pi = i32::try_from(p).map_err(|e| e.to_string())?;
    let (next, prev) = ((r + 1) % pi, (r + pi - 1) % pi);
    // reverse-recv
    let a = c.irecv::<i32>(2, prev, Tag(19)).map_err(e2s)?;
    let b = c.irecv::<i32>(2, prev, Tag(19)).map_err(e2s)?;
    let s1 = c.isend(vec![11, 22], next, Tag(19)).map_err(e2s)?;
    let s2 = c.isend(vec![33, 44], next, Tag(19)).map_err(e2s)?;
    let second = b.wait_received().map_err(e2s)?;
    let first = a.wait_received().map_err(e2s)?;
    s1.wait().map_err(e2s)?;
    s2.wait().map_err(e2s)?;
    ensure!(
        first == [11, 22] && second == [33, 44],
        "posting order violated: first-posted={first:?} second-posted={second:?}"
    );
    // preposted-recv: blocking synchronous sends complete thanks to the
    // posted receives (a ring, at any rank count including 2).
    let cr = world(LEVEL, Some(op_timeout()), SendMode::Rendezvous);
    let recv = cr.irecv::<i32>(1, prev, Tag(71)).map_err(e2s)?;
    let t0 = Instant::now();
    cr.send_mode(&[r], next, Tag(71), SendMode::Rendezvous)
        .map_err(e2s)?;
    ensure!(
        t0.elapsed() < FAST,
        "synchronous send waited for a deadline"
    );
    ensure!(recv.wait().map_err(e2s)? == vec![prev]);
    // drop-recv: an unmatched dropped receive is cancelled (made
    // deterministic by a barrier before the sends).
    drop(c.irecv::<i32>(1, prev, Tag(21)).map_err(e2s)?);
    c.barrier().map_err(e2s)?;
    let s = c
        .isend_mode(vec![r], next, Tag(21), SendMode::Rendezvous)
        .map_err(e2s)?;
    ensure!(c.receive_vec::<i32>(prev, Tag(21)).map_err(e2s)? == vec![prev]);
    s.wait().map_err(e2s)?;
    // A message matched by a posted receive is discarded when it is dropped.
    let matched = c.irecv::<i32>(1, prev, Tag(22)).map_err(e2s)?;
    c.barrier().map_err(e2s)?;
    c.isend_mode(vec![r], next, Tag(22), SendMode::Rendezvous)
        .map_err(e2s)?
        .wait()
        .map_err(e2s)?;
    c.barrier().map_err(e2s)?;
    drop(matched);
    ensure!(c.iprobe::<i32>(Some(prev), Tag(22)).map_err(e2s)?.is_none());
    // Truncation: an ordinary error, the communicator stays healthy —
    // one byte too long and far too long (the whole message is received at
    // its exact length and reported as truncated by akantu).
    for (tag, msg) in [(23, vec![1u8, 2]), (24, vec![1u8, 2, 3, 4, 5, 6, 7, 8])] {
        let rq = c.irecv::<u8>(1, prev, Tag(tag)).map_err(e2s)?;
        c.send(&msg, next, Tag(tag)).map_err(e2s)?;
        let e = rq.wait_received();
        ensure!(
            e.as_ref()
                .is_err_and(|e| e.fatal_kind().is_none() && e.to_string().contains("truncated")),
            "{} bytes into 1: {e:?}",
            msg.len()
        );
    }
    c.barrier().map_err(e2s)?;
    ensure!(c.broken().is_none() && cr.broken().is_none());
    Ok(())
}

/// Size overflow is an error, never a panic or
/// a huge allocation — a record end, an `irecv` length, and a
/// rank-specific overflowing `nb_data` in a synchronizer (agreed on every
/// rank before anything is allocated or posted).
fn cm06_rank_specific_overflow() -> Outcome {
    let c = fresh();
    let (r, p) = (c.rank(), c.size());
    let pi = i32::try_from(p).map_err(|e| e.to_string())?;
    let overflow = std::panic::catch_unwind(|| {
        let mut b = CommunicationBuffer::new();
        akantu_comm::accessor::pack_entity_data::<u8>(&[], usize::MAX, &mut b, &[1])
    });
    ensure!(
        matches!(overflow, Ok(Err(_))),
        "pack_entity_data overflow: {overflow:?}"
    );
    ensure!(c.irecv::<f64>(usize::MAX / 4, 0, Tag(5)).is_err());
    ensure!(
        c.irecv::<u8>(usize::try_from(i32::MAX).unwrap_or(0) + 1, 0, Tag(5))
            .is_err()
    );
    for (bad, nb) in [(0, usize::MAX / 2), (pi - 1, 1usize << 40)] {
        let t0 = Instant::now();
        let mut s: Synchronizer<Idx> = Synchronizer::new("ovf", c.clone());
        s.communications.send.insert((r + 1) % pi, vec![0, 1]);
        s.communications.recv.insert((r + pi - 1) % pi, vec![2]);
        let mut data = vec![f64::from(r); 3];
        let nb_component = if r == bad { nb } else { 1 };
        let mut acc =
            SimpleIdxDataAccessor::new(&mut data, nb_component, SynchronizationTag::Whatever);
        let e = s.synchronize(&mut acc, SynchronizationTag::Whatever);
        ensure!(
            e.as_ref().is_err_and(|e| e.fatal_kind().is_none()),
            "rank {r} (bad {bad}): {e:?}"
        );
        ensure!(t0.elapsed() < FAST);
    }
    let mut v = [1u8];
    c.all_reduce(&mut v, ReduceOp::Sum).map_err(e2s)?;
    ensure!(usize::from(v[0]) == p && c.broken().is_none());
    Ok(())
}

/// Control case `two-async` (must pass): one asynchronous ring
/// synchronizer on a parent and one on its duplicate, completed in reverse
/// order — independent ghost data, no shared scope/tag state.
fn two_async_parent_and_duplicate() -> Outcome {
    let c = fresh();
    let (r, p) = (c.rank(), c.size());
    let pi = i32::try_from(p).map_err(|e| e.to_string())?;
    let d = c.duplicate().map_err(e2s)?;
    let mut a: Synchronizer<Idx> = Synchronizer::new("a", c.clone());
    let mut b: Synchronizer<Idx> = Synchronizer::new("b", d.clone());
    for s in [&mut a, &mut b] {
        s.communications.send.insert((r + 1) % pi, vec![0]);
        s.communications.recv.insert((r + pi - 1) % pi, vec![1]);
    }
    let mut x = [r, -1];
    let mut y = [r + 100, -1];
    let tag = SynchronizationTag::Test;
    {
        let mut ax = SimpleIdxDataAccessor::new(&mut x, 1, tag);
        let mut by = SimpleIdxDataAccessor::new(&mut y, 1, tag);
        a.asynchronous_synchronize(&ax, tag).map_err(e2s)?;
        b.asynchronous_synchronize(&by, tag).map_err(e2s)?;
        b.wait_end_synchronize(&mut by, tag).map_err(e2s)?;
        a.wait_end_synchronize(&mut ax, tag).map_err(e2s)?;
    }
    let prev = (r + pi - 1) % pi;
    ensure!(x[1] == prev && y[1] == prev + 100, "x={x:?} y={y:?}");
    Ok(())
}

/// One native-policy comparison against rsmpi's own `MPI_Allreduce`.
fn native_vs_rsmpi<T>(
    nc: &Communicator,
    w: &mpi::topology::SimpleCommunicator,
    x: &[T],
    op: ReduceOp,
    sys: mpi::collective::SystemOperation,
) -> std::result::Result<bool, String>
where
    T: akantu_comm::CommData + mpi::datatype::Equivalence + PartialEq + Default,
{
    use mpi::traits::CommunicatorCollectives;
    let mut ours = x.to_vec();
    nc.all_reduce(&mut ours, op).map_err(e2s)?;
    let mut theirs = vec![T::default(); x.len()];
    w.all_reduce_into(x, &mut theirs[..], sys);
    Ok(ours == theirs)
}

/// D-039: the `NativeMpi` policy uses MPI's typed predefined
/// reductions consistently for all-reduce, rooted reduce, inclusive and
/// exclusive scan and min/max-loc; it is inherited by duplicates and
/// splits; a policy mismatch between ranks fails on every rank. Results
/// equal rsmpi's own native collectives on the same group bitwise (same
/// MPI implementation, datatype, count and operator). Prints the
/// order-sensitive sum under both policies (provenance).
fn cm08_native_policy() -> Outcome {
    use mpi::collective::SystemOperation as S;
    use mpi::traits::{Communicator as _, CommunicatorCollectives, Root};
    let nc = world_with_policy(LEVEL, Some(op_timeout()), ReductionPolicy::NativeMpi);
    let rc = fresh();
    let (r, p) = (nc.rank(), nc.size());
    let rf = f64::from(r);
    ensure!(nc.reduction_policy() == ReductionPolicy::NativeMpi);
    ensure!(rc.reduction_policy() == ReductionPolicy::RankOrdered);
    ensure!(nc.duplicate().map_err(e2s)?.reduction_policy() == ReductionPolicy::NativeMpi);
    let split = nc.split(Some(0), -r).map_err(e2s)?.ok_or("member")?;
    ensure!(split.reduction_policy() == ReductionPolicy::NativeMpi);
    let w = mpi::topology::SimpleCommunicator::world();
    let x = ORDER_SENSITIVE[usize::try_from(r).unwrap_or(0) % 4];
    let checks = [
        native_vs_rsmpi(
            &nc,
            &w,
            &[x, rf * 0.3 + 0.1, 1.0 / (rf + 3.0)],
            ReduceOp::Sum,
            S::sum(),
        )?,
        native_vs_rsmpi(
            &nc,
            &w,
            &[1.0 + rf * 1e-8, 0.5f64.powi(r)],
            ReduceOp::Prod,
            S::product(),
        )?,
        native_vs_rsmpi(&nc, &w, &[-rf, rf], ReduceOp::Min, S::min())?,
        native_vs_rsmpi(&nc, &w, &[-rf, rf], ReduceOp::Max, S::max())?,
        native_vs_rsmpi(
            &nc,
            &w,
            &[0.1f32 * (r as f32 + 1.0)],
            ReduceOp::Sum,
            S::sum(),
        )?,
        native_vs_rsmpi(&nc, &w, &[r * 7, -r, 1], ReduceOp::Sum, S::sum())?,
        native_vs_rsmpi(
            &nc,
            &w,
            &[0xF0F0u64 ^ (r as u64 * 3)],
            ReduceOp::BitXor,
            S::bitwise_xor(),
        )?,
        native_vs_rsmpi(
            &nc,
            &w,
            &[u8::from(r % 2 == 0)],
            ReduceOp::LogicalAnd,
            S::logical_and(),
        )?,
        native_vs_rsmpi(
            &nc,
            &w,
            &[i8::try_from(r % 100).unwrap_or(0) - 3],
            ReduceOp::Max,
            S::max(),
        )?,
        native_vs_rsmpi(&nc, &w, &[r as u32 | 1], ReduceOp::BitOr, S::bitwise_or())?,
        native_vs_rsmpi(&nc, &w, &[i64::from(r) - 1], ReduceOp::Prod, S::product())?,
    ];
    ensure!(
        checks.iter().all(|&b| b),
        "native all_reduce differs from MPI_Allreduce: {checks:?}"
    );
    // Rooted reduce (last rank).
    let root = i32::try_from(p - 1).unwrap_or(0);
    let mut ours = [x, rf];
    nc.reduce(&mut ours, ReduceOp::Sum, root).map_err(e2s)?;
    let mut theirs = [0.0f64; 2];
    if r == root {
        w.process_at_rank(root)
            .reduce_into_root(&[x, rf][..], &mut theirs[..], S::sum());
        ensure!(
            ours.map(f64::to_bits) == theirs.map(f64::to_bits),
            "reduce {ours:?} {theirs:?}"
        );
    } else {
        w.process_at_rank(root).reduce_into(&[x, rf][..], S::sum());
        ensure!(ours == [x, rf], "non-root unchanged");
    }
    // Scans.
    let v = [rf * 0.1 + 1e15 * if r % 2 == 0 { 1.0 } else { -1.0 }, rf];
    let mut ours = v;
    nc.scan(&mut ours, ReduceOp::Sum).map_err(e2s)?;
    let mut theirs = [0.0f64; 2];
    w.scan_into(&v[..], &mut theirs[..], S::sum());
    ensure!(ours.map(f64::to_bits) == theirs.map(f64::to_bits), "scan");
    let mut ours = v;
    nc.exclusive_scan(&mut ours, ReduceOp::Sum).map_err(e2s)?;
    let mut theirs = [0.0f64; 2];
    w.exclusive_scan_into(&v[..], &mut theirs[..], S::sum());
    if r == 0 {
        ensure!(ours == [0.0, 0.0], "exclusive scan: rank 0 gets zero");
    } else {
        ensure!(ours.map(f64::to_bits) == theirs.map(f64::to_bits), "exscan");
    }
    // Loc (MPI_DOUBLE_INT, MPI_MINLOC/MAXLOC): MPI-defined semantics on
    // finite values — ties keep the smallest location.
    let mut loc = [
        ValueLoc {
            value: f64::from(r % 2),
            loc: 100 - r,
        },
        ValueLoc { value: -rf, loc: r },
    ];
    let mut maxloc = loc;
    nc.all_reduce_loc(&mut loc, ReduceOp::MinLoc).map_err(e2s)?;
    nc.all_reduce_loc(&mut maxloc, ReduceOp::MaxLoc)
        .map_err(e2s)?;
    let pi = i32::try_from(p).unwrap_or(1);
    let last_even = if (pi - 1) % 2 == 0 { pi - 1 } else { pi - 2 };
    ensure!(
        loc[0]
            == ValueLoc {
                value: 0.0,
                loc: 100 - last_even
            },
        "minloc tie {:?}",
        loc[0]
    );
    ensure!(
        loc[1]
            == ValueLoc {
                value: -f64::from(pi - 1),
                loc: pi - 1
            },
        "{:?}",
        loc[1]
    );
    let max0 = if p > 1 {
        ValueLoc {
            value: 1.0,
            loc: 100 - (pi - 1 - (pi % 2 == 1) as i32),
        }
    } else {
        ValueLoc {
            value: 0.0,
            loc: 100,
        }
    };
    ensure!(
        maxloc[0] == max0,
        "maxloc tie {:?} expected {max0:?}",
        maxloc[0]
    );
    ensure!(
        maxloc[1] == ValueLoc { value: 0.0, loc: 0 },
        "{:?}",
        maxloc[1]
    );
    // Provenance of the order-sensitive sum under both policies.
    let mut native = [x];
    nc.all_reduce(&mut native, ReduceOp::Sum).map_err(e2s)?;
    let mut ordered = [x];
    rc.all_reduce(&mut ordered, ReduceOp::Sum).map_err(e2s)?;
    if r == 0 {
        println!(
            "DIAG reduction-policy: ranks={p} data=[1e16,1,-1e16,1][rank%4] rank-ordered={:e} native-mpi={:e} bitwise-equal={}",
            ordered[0],
            native[0],
            ordered[0].to_bits() == native[0].to_bits()
        );
    }
    // A policy mismatch between ranks fails on every rank, promptly.
    let mixed = world_with_policy(
        LEVEL,
        Some(op_timeout()),
        if r == 0 {
            ReductionPolicy::NativeMpi
        } else {
            ReductionPolicy::RankOrdered
        },
    );
    let t0 = Instant::now();
    let e = mixed.all_reduce(&mut [1.0f64], ReduceOp::Sum);
    // Every collective carries the policy: a mixed communicator cannot
    // communicate at all (not even a barrier), on every rank.
    let b = mixed.barrier();
    if p > 1 {
        for (what, e) in [("all_reduce", &e), ("barrier", &b)] {
            ensure!(
                e.as_ref()
                    .is_err_and(|e| e.fatal_kind().is_none() && e.to_string().contains("policy")),
                "policy mismatch ({what}): {e:?}"
            );
        }
        ensure!(t0.elapsed() < FAST && mixed.broken().is_none());
    } else {
        ensure!(e.is_ok() && b.is_ok());
    }
    // Signed-zero ties are call-form dependent in native MPI: upstream's
    // in-place `MPI_Allreduce` (native policy) versus rsmpi's separate
    // buffers (reported, not asserted).
    let z = if r % 2 == 0 { 0.0f64 } else { -0.0 };
    let mut ours = [z];
    nc.all_reduce(&mut ours, ReduceOp::Min).map_err(e2s)?;
    let mut theirs = 0.0f64;
    w.all_reduce_into(&z, &mut theirs, S::min());
    if r == 0 {
        println!(
            "DIAG signed-zero MIN: ranks={p} native-in-place={:016x} rsmpi-separate-buffers={:016x}",
            ours[0].to_bits(),
            theirs.to_bits()
        );
    }
    Ok(())
}

/// D-039: exceptional values (±0, NaN, ±inf) and MIN/MAX/loc under
/// both policies. Rank-ordered: the exact documented results, bitwise
/// (`communicator::tests::rank_ordered_exceptional_values`). Native: only
/// what IEEE 754 and MPI define (NaN/inf propagation in sums, infinities in
/// MIN/MAX, loc ties); implementation-defined results (sign of a zero
/// minimum, MIN/MAX with a NaN operand) are reported as `DIAG`, never
/// asserted.
fn cm08_exceptional_values() -> Outcome {
    let (r, p) = {
        let c = fresh();
        (c.rank(), c.size())
    };
    let ru = usize::try_from(r).unwrap_or(0);
    // Patterns indexed by rank % 3.
    let pick = |v: [f64; 3]| v[ru % 3];
    let data = [
        pick([0.0, -0.0, 0.0]),
        pick([-0.0, 0.0, -0.0]),
        pick([1.0, f64::NAN, 2.0]),
        pick([f64::NAN, 1.0, 2.0]),
        pick([f64::INFINITY, 1.0, f64::NEG_INFINITY]),
        pick([5.0, f64::INFINITY, 3.0]),
    ];
    for policy in [ReductionPolicy::RankOrdered, ReductionPolicy::NativeMpi] {
        let c = world_with_policy(LEVEL, Some(op_timeout()), policy);
        let (mut sum, mut mn, mut mx) = (data, data, data);
        c.all_reduce(&mut sum, ReduceOp::Sum).map_err(e2s)?;
        c.all_reduce(&mut mn, ReduceOp::Min).map_err(e2s)?;
        c.all_reduce(&mut mx, ReduceOp::Max).map_err(e2s)?;
        let nan_present = p > 1;
        // IEEE: a NaN operand gives NaN; inf + (-inf) = NaN; +inf + finite.
        if nan_present {
            ensure!(
                sum[2].is_nan() && sum[3].is_nan(),
                "{policy}: NaN sum {sum:?}"
            );
            ensure!(sum[5] == f64::INFINITY, "{policy}: {sum:?}");
        }
        if p > 2 {
            ensure!(sum[4].is_nan(), "{policy}: inf - inf {sum:?}");
            ensure!(
                mn[4] == f64::NEG_INFINITY && mx[4] == f64::INFINITY,
                "{policy}"
            );
        }
        ensure!(
            sum[0] == 0.0 && mn[0] == 0.0 && mx[0] == 0.0,
            "{policy}: zeros"
        );
        if p > 1 {
            ensure!(
                mx[5] == f64::INFINITY && mn[5] == pick_min(p),
                "{policy}: {mn:?} {mx:?}"
            );
        }
        if policy == ReductionPolicy::RankOrdered && p > 1 {
            // Exact rank-ordered conventions (module docs).
            ensure!(mn[0].to_bits() == 0.0f64.to_bits(), "tie keeps +0");
            ensure!(mn[1].to_bits() == (-0.0f64).to_bits(), "tie keeps -0");
            ensure!(
                mn[2] == 1.0 && mx[2] == if p > 2 { 2.0 } else { 1.0 },
                "NaN never replaces"
            );
            ensure!(mn[3].is_nan() && mx[3].is_nan(), "a NaN accumulator stays");
        }
        if r == 0 {
            println!(
                "DIAG exceptional policy={policy} ranks={p} min={:?} max={:?} (bits min[0..4]={:016x?})",
                mn,
                mx,
                mn[..4].iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
        }
        // Loc ties: the smallest location (MPI-defined; rank order alike).
        let mut loc = [ValueLoc {
            value: 7.0,
            loc: 50 - r,
        }];
        let mut maxloc = loc;
        c.all_reduce_loc(&mut loc, ReduceOp::MinLoc).map_err(e2s)?;
        c.all_reduce_loc(&mut maxloc, ReduceOp::MaxLoc)
            .map_err(e2s)?;
        let pi = i32::try_from(p).unwrap_or(1);
        ensure!(
            loc[0]
                == ValueLoc {
                    value: 7.0,
                    loc: 51 - pi
                },
            "{policy}: {:?}",
            loc[0]
        );
        ensure!(maxloc[0] == loc[0], "{policy}: {:?}", maxloc[0]);
    }
    Ok(())
}

/// Minimum of the last data column over the ranks present.
fn pick_min(p: usize) -> f64 {
    [5.0, f64::INFINITY, 3.0]
        .iter()
        .take(p.min(3))
        .copied()
        .fold(f64::INFINITY, f64::min)
}

// ------------------------------------------------------------- main

fn main() {
    start_watchdog(900);
    let control = world(LEVEL, Some(op_timeout()), SendMode::Eager);
    let env = mpi_lifecycle();
    let provided = mpi::environment::threading_support();
    let owned = !env.is_finalized() && {
        // Rust-first program: akantu owns MPI.
        let g = env.initialize(ThreadLevel::Single).expect("guard");
        g.ownership() == InitOwnership::Owned
    };
    if control.rank() == 0 {
        println!(
            "INFO mpi_acceptance: ranks={} provided={provided:?} owned={owned} default-policy={} library={}",
            control.size(),
            control.reduction_policy(),
            mpi::environment::library_version()
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or("")
        );
    }
    let mut h = Harness::new(control);
    h.run("upstream_communicator_bcast", upstream_bcast);
    h.run("upstream_communicator_receive_any", upstream_receive_any);
    h.run(
        "receive_any_same_tag_diagnostic",
        receive_any_same_tag_diagnostic,
    );
    h.run("serial_local_mpi_equivalence", serial_local_mpi_equivalence);
    h.run(
        "native_allreduce_order_diagnostic",
        native_allreduce_order_diagnostic,
    );
    h.run("communicator_api", communicator_api);
    h.run("checked_collectives", checked_collectives);
    h.run("split_and_duplicate", split_and_duplicate);
    h.run("phase_injected_failures", phase_injected_failures);
    h.run("phase_failure_sweep", phase_failure_sweep);
    h.run("phase_protocol_ports", phase_protocol_ports);
    h.run("park_resume_on_workers", park_resume_on_workers);
    h.run(
        "upstream_synchronizer_communication",
        upstream_synchronizer_communication,
    );
    h.run(
        "synchronizer_reduction_and_failures",
        synchronizer_reduction_and_failures,
    );
    h.run("large_rendezvous_exchanges", large_rendezvous_exchanges);
    h.run(
        "gather_scatter_and_array_sync",
        gather_scatter_and_array_sync,
    );
    h.run("sanity_check_hook", sanity_check_hook);
    h.run(
        "cm01_intercommunicator_import",
        cm01_intercommunicator_import,
    );
    h.run("cm02_collective_contract", cm02_collective_contract);
    h.run("cm05_posted_receives", cm05_posted_receives);
    h.run("cm06_rank_specific_overflow", cm06_rank_specific_overflow);
    h.run(
        "two_async_parent_and_duplicate",
        two_async_parent_and_duplicate,
    );
    h.run("cm08_native_policy", cm08_native_policy);
    h.run("cm08_exceptional_values", cm08_exceptional_values);
    let code = h.finish();
    // Last guards dropped on the main thread: akantu finalises MPI.
    let finalized = env.is_finalized();
    let stats = akantu_comm::mpi_backend::handle_stats();
    println!(
        "INFO lifecycle: finalized={finalized} comms_created={} comms_freed={} freed_at_finalize={:?}",
        stats.comms_created, stats.comms_freed, stats.freed_at_finalize
    );
    let code = if owned && (!finalized || stats.freed_at_finalize != Some(stats.comms_created)) {
        eprintln!("lifecycle check failed: every communicator must be freed before MPI_Finalize");
        1
    } else {
        code
    };
    std::process::exit(code);
}
