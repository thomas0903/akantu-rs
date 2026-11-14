// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs addition (no upstream counterpart; Akantu by EPFL/LSMS waits in
// blocking MPI calls, src/synchronizer/communicator_mpi_inline_impl.hh):
// pacing of the polled waits of the MPI backend and of the self-message
// channel.

//! Pacing of polled waits.
//!
//! The MPI backend never blocks inside MPI: every wait polls (`MPI_Test`,
//! `MPI_Improbe`) so that it can honour its progress deadline, release the
//! `Serialized` MPI lock between polls and run the posted-receive matching
//! engine. What a waiting thread does **between** two polls decides both the
//! latency and the CPU cost of a wait. A [`Pacer`] is created per wait (per
//! operation phase), so its state starts afresh with every operation:
//!
//! 1. **Spin** ([`SPIN_POLLS`] polls with the `spin_loop` hint): the
//!    completion of a short operation is seen at once.
//! 2. **Yield** (`sched_yield`) between polls until the wait has lasted
//!    [`YIELD_FOR`]: the polling cadence of `MPI_Wait` for the operations
//!    that dominate a halo exchange or a rendezvous pipeline, while other
//!    runnable threads or processes (oversubscribed runs) get the core.
//! 3. **Sleep** [`MAX_PAUSE`] between polls once the wait has lasted
//!    [`YIELD_FOR`] (about 300 µs with Linux's default 50 µs timer slack,
//!    i.e. at most about 1.5 % added latency on such a wait), so an idle
//!    wait does not burn a core (a poll costs a few µs: about 1–2 % of a
//!    core); a pause never extends past the wait's deadline.
//!
//! The earlier back-off slept 100 µs per poll after 1024 polls of a wait:
//! large shared-memory messages (many progress steps) became 40–600× slower
//! than raw MPI (`docs/BENCHMARKS.md`).
//!
//! The same schedule gives the slice length of a condition-variable wait
//! that must run progress work between slices ([`Pacer::slice`]: the
//! self-message channel): [`MIN_PAUSE`] during [`YIELD_FOR`], then
//! [`MAX_PAUSE`]. Such a wait restarts its pacer ([`Pacer::restart`])
//! whenever its progress work reports pending MPI work, so it keeps the
//! short slices — the progress cadence of a blocking MPI call — while other
//! operations of the process need progress.

use std::time::{Duration, Instant};

/// Polls of a wait that spin (`spin_loop` hint) before yielding.
pub(crate) const SPIN_POLLS: u32 = 64;
/// Wait time during which polls yield the core instead of sleeping.
pub(crate) const YIELD_FOR: Duration = Duration::from_millis(20);
/// Short condition-variable slice (and the shortest sleep, near a
/// deadline). Linux rounds shorter sleeps up to its timer slack (50 µs by
/// default) anyway.
pub(crate) const MIN_PAUSE: Duration = Duration::from_micros(50);
/// Sleep between polls (and condition-variable slice) after [`YIELD_FOR`].
pub(crate) const MAX_PAUSE: Duration = Duration::from_micros(250);

/// What a waiting thread does before its next poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pause {
    /// Busy-wait hint (`std::hint::spin_loop`).
    Spin,
    /// `std::thread::yield_now`.
    Yield,
    /// `std::thread::sleep` for the given time.
    Sleep(Duration),
}

/// Poll pacing of one wait (see the module documentation).
#[derive(Debug)]
pub(crate) struct Pacer {
    start: Instant,
    polls: u32,
}

impl Pacer {
    /// The pacing of a wait starting now.
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
            polls: 0,
        }
    }

    /// Start the schedule afresh (pending work was observed).
    pub(crate) fn restart(&mut self) {
        *self = Self::new();
    }

    /// `d`, but never past `deadline`.
    fn until(d: Duration, now: Instant, deadline: Option<Instant>) -> Duration {
        deadline.map_or(d, |dl| d.min(dl.saturating_duration_since(now)))
    }

    /// The pause before the next poll of a wait that has polled once more,
    /// at time `now`, with the wait's `deadline`.
    pub(crate) fn next(&mut self, now: Instant, deadline: Option<Instant>) -> Pause {
        self.polls = self.polls.saturating_add(1);
        if self.polls <= SPIN_POLLS {
            return Pause::Spin;
        }
        if now.saturating_duration_since(self.start) < YIELD_FOR {
            return Pause::Yield;
        }
        match Self::until(MAX_PAUSE, now, deadline) {
            d if d.is_zero() => Pause::Yield,
            d => Pause::Sleep(d),
        }
    }

    /// Pause before the next poll (see [`Pacer::next`]).
    pub(crate) fn pause(&mut self, deadline: Option<Instant>) {
        match self.next(Instant::now(), deadline) {
            Pause::Spin => std::hint::spin_loop(),
            Pause::Yield => std::thread::yield_now(),
            Pause::Sleep(d) => std::thread::sleep(d),
        }
    }

    /// Length of the next slice of a condition-variable wait that runs
    /// progress work between slices: [`MIN_PAUSE`] during [`YIELD_FOR`],
    /// then [`MAX_PAUSE`]; never past `deadline` (zero once it passed).
    pub(crate) fn slice(&self, now: Instant, deadline: Option<Instant>) -> Duration {
        let d = if now.saturating_duration_since(self.start) < YIELD_FOR {
            MIN_PAUSE
        } else {
            MAX_PAUSE
        };
        Self::until(d, now, deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spin_then_yield_then_short_sleeps() {
        let mut p = Pacer::new();
        let t0 = p.start;
        for _ in 0..SPIN_POLLS {
            assert_eq!(p.next(t0, None), Pause::Spin);
        }
        // Short waits never sleep, however many polls they make.
        for k in 0..100_000u32 {
            let now = t0 + YIELD_FOR * k / 100_001;
            assert_eq!(p.next(now, None), Pause::Yield);
        }
        // Then short sleeps, however long the wait.
        assert_eq!(p.next(t0 + YIELD_FOR, None), Pause::Sleep(MAX_PAUSE));
        assert_eq!(
            p.next(t0 + Duration::from_secs(5), None),
            Pause::Sleep(MAX_PAUSE)
        );
    }

    #[test]
    fn sleeps_never_pass_the_deadline() {
        let mut p = Pacer::new();
        let t0 = p.start;
        p.polls = SPIN_POLLS;
        let now = t0 + Duration::from_secs(1);
        let dl = now + Duration::from_micros(7);
        assert_eq!(
            p.next(now, Some(dl)),
            Pause::Sleep(Duration::from_micros(7))
        );
        // At or after the deadline: no sleep (the caller checks it next).
        assert_eq!(p.next(now, Some(now)), Pause::Yield);
        assert_eq!(p.next(now, Some(t0)), Pause::Yield);
    }

    #[test]
    fn a_new_pacer_starts_afresh() {
        let mut p = Pacer::new();
        p.polls = 1_000_000;
        let late = p.start + Duration::from_secs(3);
        assert!(matches!(p.next(late, None), Pause::Sleep(_)));
        let mut q = Pacer::new();
        assert_eq!(q.next(q.start, None), Pause::Spin);
        p.restart();
        let t = p.start;
        assert_eq!(p.next(t, None), Pause::Spin);
        assert_eq!(p.slice(t, None), MIN_PAUSE);
    }

    #[test]
    fn slices_are_short_first_then_longer_and_respect_the_deadline() {
        let p = Pacer::new();
        let t0 = p.start;
        assert_eq!(p.slice(t0, None), MIN_PAUSE);
        assert_eq!(p.slice(t0 + YIELD_FOR / 2, None), MIN_PAUSE);
        assert_eq!(p.slice(t0 + YIELD_FOR, None), MAX_PAUSE);
        assert_eq!(p.slice(t0 + Duration::from_secs(9), None), MAX_PAUSE);
        let now = t0 + Duration::from_secs(9);
        assert_eq!(
            p.slice(now, Some(now + Duration::from_micros(3))),
            Duration::from_micros(3)
        );
        assert_eq!(p.slice(now, Some(t0)), Duration::ZERO);
    }
}
