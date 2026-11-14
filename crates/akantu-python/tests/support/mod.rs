// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test support (no upstream counterpart): hang-proof
// synchronisation for the multi-threaded foreign-view protocol tests.

//! Multi-threaded tests that **fail instead of hanging**.
//!
//! A plain `std::sync::Barrier` turns any panic of one party into a hang of
//! the others (they wait forever, and `std::thread::scope` then waits for
//! them forever). Here:
//!
//! * [`Team`] is a reusable rendezvous of a fixed number of parties. A wait
//!   panics after [`DEADLINE`], or at once when the team is *broken*: every
//!   party holds a [`Member`] guard that breaks the team if its thread
//!   unwinds, so the peers of a failing thread fail promptly with a message
//!   naming the step, and the original panic is what the test reports.
//! * [`watchdog`] runs a whole test body on its own thread and fails the
//!   test if the body has not finished after [`DEADLINE`] — whatever it is
//!   blocked on (rendezvous, lock, spin loop). The stuck thread is left
//!   behind; the test binary still terminates.

#![allow(dead_code)] // each test target uses a subset of the helpers

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Upper bound for one rendezvous wait and for a whole watched test. Under
/// Miri the clock is virtual and advances with interpreted work, so the
/// bound is raised there; a genuine deadlock still ends (Miri jumps the
/// virtual clock to the timeout once every thread is blocked).
pub const DEADLINE: Duration = if cfg!(miri) {
    Duration::from_secs(24 * 3600)
} else {
    Duration::from_secs(30)
};

#[derive(Debug, Default)]
struct TeamState {
    arrived: usize,
    generation: u64,
    broken: bool,
}

/// Reusable rendezvous of `parties` threads (see the module docs).
#[derive(Debug)]
pub struct Team {
    parties: usize,
    state: Mutex<TeamState>,
    cv: Condvar,
}

impl Team {
    /// A team of `parties` threads.
    #[must_use]
    pub fn new(parties: usize) -> Self {
        assert!(parties > 0);
        Self {
            parties,
            state: Mutex::new(TeamState::default()),
            cv: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, TeamState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Register the calling thread: the team breaks if it unwinds while the
    /// guard is alive.
    #[must_use]
    pub fn member(&self) -> Member<'_> {
        Member(self)
    }

    /// `true` once a member panicked.
    pub fn is_broken(&self) -> bool {
        self.lock().broken
    }

    /// Panics if a member panicked (for spin loops between rendezvous).
    pub fn check(&self, step: &str) {
        assert!(
            !self.is_broken(),
            "step '{step}': another party of the test panicked"
        );
    }

    /// Wait until all parties reached this step. Panics on timeout or if
    /// the team is (or becomes) broken.
    pub fn wait(&self, step: &str) {
        let mut st = self.lock();
        assert!(!st.broken, "step '{step}': another party panicked");
        let generation = st.generation;
        st.arrived += 1;
        if st.arrived == self.parties {
            st.arrived = 0;
            st.generation += 1;
            self.cv.notify_all();
            return;
        }
        let (st, timeout) = self
            .cv
            .wait_timeout_while(st, DEADLINE, |s| s.generation == generation && !s.broken)
            .unwrap_or_else(PoisonError::into_inner);
        if st.generation != generation {
            return;
        }
        assert!(!st.broken, "step '{step}': another party panicked");
        assert!(
            !timeout.timed_out(),
            "step '{step}': rendezvous timed out after {DEADLINE:?} (a party is stuck)"
        );
        unreachable!("condvar returned without generation change, break or timeout");
    }
}

/// Membership guard of a [`Team`]: breaks it when dropped during a panic.
#[derive(Debug)]
pub struct Member<'a>(&'a Team);

impl Drop for Member<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.lock().broken = true;
            self.0.cv.notify_all();
        }
    }
}

/// Run `body` on a fresh thread; fail if it panics (re-raised here) or has
/// not finished after [`DEADLINE`] (hang).
pub fn watchdog<R: Send + 'static>(body: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = mpsc::channel();
    let name = std::thread::current()
        .name()
        .map_or_else(|| "watched".to_owned(), str::to_owned);
    let handle = std::thread::Builder::new()
        .name(name.clone())
        .spawn(move || {
            let r = body();
            let _ = tx.send(r);
        })
        .expect("spawn the watched test body");
    match rx.recv_timeout(DEADLINE) {
        Ok(r) => {
            handle.join().expect("the body finished after sending");
            r
        }
        Err(RecvTimeoutError::Disconnected) => match handle.join() {
            Err(payload) => std::panic::resume_unwind(payload),
            Ok(()) => unreachable!("the body sends its result before returning"),
        },
        Err(RecvTimeoutError::Timeout) => {
            panic!("{name}: still running after {DEADLINE:?} — hung (body thread left behind)")
        }
    }
}

/// Deadline for a spin loop that polls a condition another thread must
/// establish (diagnostics instead of a silent watchdog timeout).
#[derive(Debug, Clone, Copy)]
pub struct SpinDeadline(Instant);

impl SpinDeadline {
    /// Half of [`DEADLINE`] from now (leaves the watchdog room to report).
    #[must_use]
    pub fn start() -> Self {
        Self(Instant::now() + DEADLINE / 2)
    }

    /// Panics with `state()` if the deadline passed.
    pub fn check(&self, what: &str, state: impl FnOnce() -> String) {
        assert!(
            Instant::now() < self.0,
            "{what}: condition not reached after {:?}; state: {}",
            DEADLINE / 2,
            state()
        );
    }
}

/// Small deterministic PRNG (xorshift64*) for randomised interleavings.
#[derive(Debug, Clone)]
pub struct Jitter(u64);

impl Jitter {
    /// Generator for `seed` (any value; 0 is remapped).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// Next pseudo-random value.
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// `true` with probability 1/2.
    pub fn coin(&mut self) -> bool {
        self.next() >> 63 == 1
    }

    /// A random short delay: nothing, a few yields, or a short busy spin.
    pub fn pause(&mut self) {
        match self.next() % 4 {
            0 => {}
            1 => std::thread::yield_now(),
            2 => {
                for _ in 0..(self.next() % 4) {
                    std::thread::yield_now();
                }
            }
            _ => {
                for _ in 0..(self.next() % 256) {
                    std::hint::spin_loop();
                }
            }
        }
    }
}
