// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: failure scenarios of the collective error protocol under
// both collective-check modes (D-018, D-126), on the in-process local world.
//
// What D-126 promises in each: no rank waits for a rank that left (every
// rank ends in bounded time), every rank ends with an error when one rank
// failed, and the agreed severity class is the same on every rank.

use std::time::{Duration, Instant};

use akantu_comm::phase::{CollectiveChecks, Phases, Severity};
use akantu_comm::run_local;
use akantu_core::{Error, FatalKind, Result};

const MODES: [CollectiveChecks; 2] = [CollectiveChecks::EveryPhase, CollectiveChecks::CommitPoints];
/// The local world's per-operation timeout: a rank that waits this long
/// waited for a rank that left.
const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct Seen {
    result: std::result::Result<(), String>,
    fatal: Option<FatalKind>,
    elapsed: Duration,
}

fn seen(r: Result<()>, t0: Instant) -> Seen {
    Seen {
        fatal: r.as_ref().err().and_then(Error::fatal_kind),
        result: r.map_err(|e| e.to_string()),
        elapsed: t0.elapsed(),
    }
}

fn local_err(rank: i32, bad: i32) -> Result<()> {
    if rank == bad {
        Err(Error::Numerical(format!("injected on rank {rank}")))
    } else {
        Ok(())
    }
}

/// Every rank failed, without waiting for the deadline, with `fatal`.
fn all_failed(out: &[Seen], fatal: Option<FatalKind>, what: &str) {
    for (r, s) in out.iter().enumerate() {
        assert!(s.result.is_err(), "{what}: rank {r} succeeded");
        assert!(
            s.elapsed < DEADLINE / 2,
            "{what}: rank {r} waited {:?}",
            s.elapsed
        );
        assert_eq!(s.fatal, fatal, "{what}: rank {r}: {:?}", s.result);
    }
}

/// A local phase of a settling scope fails on rank 0 (deferred under commit
/// points); the scope's code then opens a scope on another communicator of
/// the same family (a duplicate: a callback with its own synchronizer).
/// Under commit points the other ranks refuse it locally (their scope holds
/// deferred outcomes, which a peer may have failed), so nobody waits on the
/// duplicate for rank 0, which left through its error.
#[test]
fn deferred_failure_then_scope_on_another_communicator() {
    for mode in MODES {
        let out = run_local(2, DEADLINE, |c| {
            c.set_collective_checks(mode);
            let d = c.duplicate().expect("duplicate");
            let t0 = Instant::now();
            let mut ph = Phases::settling(&c, "outer");
            let r = ph
                .defer("local", local_err(c.rank(), 0), Severity::Recoverable)
                .and_then(|()| {
                    let mut q = Phases::new(&d, "inner");
                    let r = q.agree("x", Ok(()), Severity::Recoverable);
                    q.settle(r)
                });
            let r = ph.settle(r);
            drop(ph);
            // both communicators still work (the failure was recoverable)
            let mut v = [1u64];
            assert!(c.all_reduce(&mut v, akantu_comm::ReduceOp::Sum).is_ok() && v[0] == 2);
            assert!(d.all_reduce(&mut v, akantu_comm::ReduceOp::Sum).is_ok() && v[0] == 4);
            seen(r, t0)
        });
        all_failed(&out, None, &format!("{mode:?}"));
        if mode == CollectiveChecks::CommitPoints {
            let e = out[1].result.as_ref().unwrap_err();
            assert!(e.contains("holds deferred outcomes"), "{e}");
        }
    }
}

/// Also a raw collective on another communicator is refused while a scope
/// of the thread holds deferred outcomes.
#[test]
fn deferred_outcomes_refuse_raw_collectives_elsewhere() {
    let out = run_local(2, DEADLINE, |c| {
        c.set_collective_checks(CollectiveChecks::CommitPoints);
        let d = c.duplicate().expect("duplicate");
        let mut ph = Phases::settling(&c, "outer");
        ph.defer("local", Ok(()), Severity::Recoverable).unwrap();
        let raw = d.all_reduce(&mut [1u64], akantu_comm::ReduceOp::Sum);
        let r = ph.settle(Ok(()));
        drop(ph);
        // settled and closed: the duplicate is free again
        let after = d.all_reduce(&mut [1u64], akantu_comm::ReduceOp::Sum);
        (raw.map_err(|e| e.to_string()), r.is_ok(), after.is_ok())
    });
    for (raw, settled, after) in out {
        let e = raw.unwrap_err();
        assert!(
            e.contains("raw all_reduce refused") && e.contains("deferred"),
            "{e}"
        );
        assert!(settled && after);
    }
}

/// A settling scope's owner drops it on the failing rank without settling
/// (the promise of `Phases::settling` broken), then every rank goes on to
/// its next operation. The dropped scope reports a failure in one last
/// agreement: the other ranks fail at once, and the next operation works.
#[test]
fn unsettled_owner_then_next_operation() {
    for mode in MODES {
        let out = run_local(2, DEADLINE, |c| {
            c.set_collective_checks(mode);
            let t0 = Instant::now();
            let mut ph = Phases::settling(&c, "op");
            let r = ph.defer("local", local_err(c.rank(), 0), Severity::Recoverable);
            let r = if r.is_err() {
                drop(ph); // early return, no settle
                r
            } else {
                let r = ph.settle(r);
                drop(ph);
                r
            };
            let first = seen(r, t0);
            let mut q = Phases::new(&c, "next-op");
            let n = q.agree("work", Ok(()), Severity::Recoverable);
            let next = q.settle(n);
            (first, next.is_ok(), c.broken().is_none())
        });
        let first: Vec<Seen> = out.iter().map(|o| o.0.clone()).collect();
        all_failed(&first, None, &format!("{mode:?}"));
        for (r, (_, next, usable)) in out.iter().enumerate() {
            assert!(next, "{mode:?}: rank {r}: the next operation failed");
            assert!(usable, "{mode:?}: rank {r}: communicator broken");
        }
    }
}

/// Ranks in different collective-check modes, all succeeding: one clear
/// agreed-fatal error on every rank naming the setting.
#[test]
fn mixed_modes() {
    let out = run_local(2, DEADLINE, |c| {
        c.set_collective_checks(if c.rank() == 0 {
            CollectiveChecks::EveryPhase
        } else {
            CollectiveChecks::CommitPoints
        });
        let t0 = Instant::now();
        let mut ph = Phases::settling(&c, "op");
        let r = ph
            .defer("local-1", Ok(()), Severity::Recoverable)
            .and_then(|()| ph.defer("local-2", Ok(()), Severity::Recoverable));
        let r = ph.settle(r);
        seen(r, t0)
    });
    all_failed(&out, Some(FatalKind::Agreed), "mixed modes");
    for s in &out {
        let e = s.result.as_ref().unwrap_err();
        assert!(
            e.contains("collective checks differ") && e.contains("AKANTU_COLLECTIVE_CHECKS"),
            "{e}"
        );
    }
}

/// Two ranks fail different deferred phases with different severities
/// (rank 0 recoverable first, rank 1 fatal later): the same agreed class on
/// every rank. Under every-phase checks the first failure stops the scope
/// (recoverable); under commit points both are deferred to the same
/// agreement, which takes the worst (agreed fatal): an agreed failure can
/// be more severe than under every-phase checks (D-126).
#[test]
fn two_failures_of_different_severity() {
    for mode in MODES {
        let out = run_local(2, DEADLINE, |c| {
            c.set_collective_checks(mode);
            let t0 = Instant::now();
            let mut ph = Phases::settling(&c, "op");
            let r = ph
                .defer("a", local_err(c.rank(), 0), Severity::Recoverable)
                .and_then(|()| ph.defer("b", local_err(c.rank(), 1), Severity::Fatal));
            let r = ph.settle(r);
            seen(r, t0)
        });
        let fatal = match mode {
            CollectiveChecks::EveryPhase => None,
            CollectiveChecks::CommitPoints => Some(FatalKind::Agreed),
        };
        all_failed(&out, fatal, &format!("{mode:?}"));
    }
}

/// Rank 2's preparation of a checked collective fails while the others
/// issue an all-gather of variable size: recoverable everywhere.
#[test]
fn failed_prepare_meets_payload_collective_of_another_kind() {
    for mode in MODES {
        let out = run_local(4, DEADLINE, |c| {
            c.set_collective_checks(mode);
            let t0 = Instant::now();
            let mut ph = Phases::new(&c, "op");
            let prepared = local_err(c.rank(), 2).map(|()| vec![0u8; 1000]);
            let r = ph
                .run_collective("gather", prepared, |c, v| c.all_gather_v(&v))
                .map(|_| ());
            let r = ph.settle(r);
            seen(r, t0)
        });
        all_failed(&out, None, &format!("{mode:?}"));
    }
}

/// A deferred failure on rank 1, then a checked reduction on every rank:
/// the reduction's header carries the deferred outcome (recoverable).
#[test]
fn deferred_failure_then_reduction() {
    for mode in MODES {
        let out = run_local(2, DEADLINE, |c| {
            c.set_collective_checks(mode);
            let t0 = Instant::now();
            let mut ph = Phases::settling(&c, "op");
            let r = ph
                .defer("local", local_err(c.rank(), 1), Severity::Recoverable)
                .and_then(|()| {
                    ph.run_collective("sum", Ok(1.0_f64), |c, s| {
                        let mut s = [s];
                        c.all_reduce(&mut s, akantu_comm::ReduceOp::Sum)?;
                        Ok(s[0])
                    })
                })
                .map(|_| ());
            let r = ph.settle(r);
            seen(r, t0)
        });
        all_failed(&out, None, &format!("{mode:?}"));
    }
}
