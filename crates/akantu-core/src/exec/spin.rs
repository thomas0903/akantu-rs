// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the thread pool of the execution policy (D-123).

//! A fixed pool of worker threads for short parallel regions.
//!
//! One region at a time: the caller publishes a job (`Fn(part)`) and the
//! number of parts, wakes the workers, and takes parts itself; parts are
//! claimed with an atomic counter. A worker joins a region only by entering
//! it while it is open; when the caller has no part left it closes the
//! region and waits only for the workers that entered, so a worker that
//! wakes late (parked during a serial stretch) skips the region instead of
//! making the caller wait for its wake-up. Between regions a worker spins
//! for a short time (regions of an explicit step follow each other within
//! microseconds), then parks on a condition variable. Measured cost of one region on 8 cores of an AMD EPYC 9R14:
//! below 1 µs at 2 and 4 threads, against 5–14 µs for a `rayon` pool
//! (`docs/benchmarks/perf-campaign/exec/README.md`).
//!
//! A region submitted while the pool runs another one (from another thread,
//! or from inside a part) runs serially on the calling thread: the parts
//! are independent, so the results are the same.

// The job is a borrowed closure handed to the workers with its lifetime
// erased: every site carries its `SAFETY:` comment and its row in
// `UNSAFE.md`.
#![allow(unsafe_code)]

use std::any::Any;
use std::cell::{Cell, UnsafeCell};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long an idle worker spins before it parks.
const SPIN_FOR: Duration = Duration::from_millis(5);

/// `state`: closed bit.
const CLOSED: u64 = 1 << 63;
/// `state`: workers inside the region (low bits).
const ACTIVE: u64 = 0xFFFF;
/// `state`: the region's generation (bits 16..63).
const GEN_MASK: u64 = (1 << 47) - 1;

/// The job of the running region: a part runner with its lifetime erased.
#[derive(Clone, Copy)]
struct JobPtr(*const (dyn Fn(usize) + Sync));

struct Shared {
    /// Bumped (SeqCst) to publish a region.
    generation: AtomicU64,
    /// Next unclaimed part of the running region.
    next: AtomicUsize,
    /// Parts of the running region.
    nb_parts: AtomicUsize,
    /// The region's generation (bits 16..63), its closed bit (63) and the
    /// number of workers inside it (bits 0..16).
    state: AtomicU64,
    /// Parked workers.
    sleepers: AtomicUsize,
    /// The running region's job; written only by the region's owner while
    /// no worker is inside a region (see [`SpinPool::run`]).
    job: UnsafeCell<Option<JobPtr>>,
    /// The first panic of a part of the running region.
    panic: Mutex<Option<Box<dyn Any + Send>>>,
    sleep: Mutex<()>,
    wake: Condvar,
}

// SAFETY: every field but `job` is `Sync`; `job` is written by the owner of
// the region (who holds `SpinPool::busy`) only while no worker is inside a
// region: before it opens the region (SeqCst store of `state`) and after it
// closed it and saw no worker inside (Acquire load of `state`); a worker
// reads it only after entering the open region (SeqCst CAS on `state`) and
// before leaving it, so writes and reads never overlap. The closure behind
// the pointer is `Sync`.
unsafe impl Sync for Shared {}
// SAFETY: as above; the pointer is only dereferenced under that protocol.
unsafe impl Send for Shared {}

thread_local! {
    /// Set while the thread runs a part of a region (nested regions run
    /// serially).
    static IN_REGION: Cell<bool> = const { Cell::new(false) };
}

/// The pool (see the module documentation).
pub(crate) struct SpinPool {
    shared: Arc<Shared>,
    workers: usize,
    /// Held by the owner of the running region.
    busy: Mutex<()>,
}

impl SpinPool {
    /// A pool of `threads` threads: the caller and `threads − 1` workers.
    pub(crate) fn new(threads: usize) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            generation: AtomicU64::new(0),
            next: AtomicUsize::new(0),
            nb_parts: AtomicUsize::new(0),
            state: AtomicU64::new(CLOSED),
            sleepers: AtomicUsize::new(0),
            job: UnsafeCell::new(None),
            panic: Mutex::new(None),
            sleep: Mutex::new(()),
            wake: Condvar::new(),
        });
        let workers = threads.saturating_sub(1);
        for w in 0..workers {
            let s = shared.clone();
            std::thread::Builder::new()
                .name(format!("akantu-exec-{threads}-{w}"))
                .spawn(move || worker(&s))?;
        }
        Ok(Self {
            shared,
            workers,
            busy: Mutex::new(()),
        })
    }

    /// Run `f(p)` for every part `p < nb_parts`, on the pool's threads, and
    /// return when all have run. Serially on the calling thread when the
    /// pool is busy or the caller is inside a part. A panic of a part is
    /// resumed on the caller after the region ended.
    pub(crate) fn run(&self, nb_parts: usize, f: &(dyn Fn(usize) + Sync)) {
        let serial = || (0..nb_parts).for_each(f);
        if self.workers == 0 || nb_parts <= 1 || IN_REGION.with(Cell::get) {
            return serial();
        }
        let Ok(_busy) = self.busy.try_lock() else {
            return serial();
        };
        let s = &*self.shared;
        // SAFETY: lifetime erasure of `f`; the pointer is published to the
        // workers below and this function returns only after it closed the
        // region, saw no worker inside, and cleared the slot, so no worker
        // dereferences it after `f`'s borrow ends.
        let erased: *const (dyn Fn(usize) + Sync) =
            unsafe { std::mem::transmute(f as *const (dyn Fn(usize) + Sync)) };
        // SAFETY: we hold `busy` and the previous region is closed with no
        // worker inside, so no worker reads `job` now.
        unsafe { *s.job.get() = Some(JobPtr(erased)) };
        s.next.store(0, Ordering::Relaxed);
        s.nb_parts.store(nb_parts, Ordering::Relaxed);
        let g = s.generation.load(Ordering::Relaxed).wrapping_add(1);
        // open the region, then announce it
        s.state.store((g & GEN_MASK) << 16, Ordering::SeqCst);
        s.generation.store(g, Ordering::SeqCst);
        if s.sleepers.load(Ordering::SeqCst) > 0 {
            let _g = s.sleep.lock().unwrap_or_else(|e| e.into_inner());
            s.wake.notify_all();
        }
        run_parts(s, f);
        // no part left: close the region, wait for the workers inside
        s.state.fetch_or(CLOSED, Ordering::SeqCst);
        while s.state.load(Ordering::Acquire) & ACTIVE != 0 {
            std::hint::spin_loop();
        }
        // SAFETY: the region is closed and no worker is inside: none reads
        // `job` until the next region opens.
        unsafe { *s.job.get() = None };
        let panic = s.panic.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(p) = panic {
            resume_unwind(p);
        }
    }
}

/// Claim and run parts of the current region until none is left.
fn run_parts(s: &Shared, f: &(dyn Fn(usize) + Sync)) {
    let n = s.nb_parts.load(Ordering::Relaxed);
    IN_REGION.with(|r| r.set(true));
    loop {
        let p = s.next.fetch_add(1, Ordering::Relaxed);
        if p >= n {
            break;
        }
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| f(p))) {
            let mut slot = s.panic.lock().unwrap_or_else(|e| e.into_inner());
            if slot.is_none() {
                *slot = Some(e);
            }
        }
    }
    IN_REGION.with(|r| r.set(false));
}

fn worker(s: &Shared) {
    // the generation at the pool's creation: a region published before
    // this thread started is not missed
    let mut seen = 0;
    loop {
        // wait for the next region: spin, then park
        let start = Instant::now();
        let mut polls = 0u32;
        loop {
            if s.generation.load(Ordering::SeqCst) != seen {
                break;
            }
            polls = polls.wrapping_add(1);
            if !polls.is_multiple_of(64) || start.elapsed() < SPIN_FOR {
                std::hint::spin_loop();
                continue;
            }
            let guard = s.sleep.lock().unwrap_or_else(|e| e.into_inner());
            s.sleepers.fetch_add(1, Ordering::SeqCst);
            let guard = s
                .wake
                .wait_while(guard, |()| s.generation.load(Ordering::SeqCst) == seen)
                .unwrap_or_else(|e| e.into_inner());
            s.sleepers.fetch_sub(1, Ordering::SeqCst);
            drop(guard);
        }
        seen = s.generation.load(Ordering::SeqCst);
        // enter the region of generation `seen` if it is still open
        let mut state = s.state.load(Ordering::SeqCst);
        let entered = loop {
            if state & CLOSED != 0 || (state >> 16) & GEN_MASK != seen & GEN_MASK {
                break false;
            }
            match s.state.compare_exchange_weak(
                state,
                state + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break true,
                Err(now) => state = now,
            }
        };
        if !entered {
            continue;
        }
        // SAFETY: this worker is inside the open region (counted in `state`):
        // its owner neither clears nor rewrites `job` before every worker
        // inside has left, so `job` holds the region's live closure.
        let job = unsafe { *s.job.get() };
        if let Some(JobPtr(f)) = job {
            // SAFETY: as above: the closure outlives the region.
            run_parts(s, unsafe { &*f });
        }
        s.state.fetch_sub(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn every_part_runs_once() {
        for threads in [1, 2, 3, 8] {
            let pool = SpinPool::new(threads).unwrap();
            for n in [0, 1, 2, 7, 100] {
                let hits: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
                pool.run(n, &|p| {
                    hits[p].fetch_add(1, Ordering::Relaxed);
                });
                assert!(hits.iter().all(|h| h.load(Ordering::Relaxed) == 1));
            }
        }
    }

    #[test]
    fn regions_after_parking_and_nested_regions() {
        let pool = SpinPool::new(4).unwrap();
        let total = AtomicU32::new(0);
        for round in 0..3 {
            if round > 0 {
                std::thread::sleep(Duration::from_millis(5)); // workers park
            }
            pool.run(16, &|_| {
                // a nested region runs serially
                pool.run(3, &|_| {
                    total.fetch_add(1, Ordering::Relaxed);
                });
            });
        }
        assert_eq!(total.load(Ordering::Relaxed), 3 * 16 * 3);
    }

    #[test]
    fn concurrent_callers() {
        let pool = SpinPool::new(4).unwrap();
        let total = AtomicU32::new(0);
        std::thread::scope(|sc| {
            for _ in 0..4 {
                sc.spawn(|| {
                    for _ in 0..200 {
                        pool.run(8, &|_| {
                            total.fetch_add(1, Ordering::Relaxed);
                        });
                    }
                });
            }
        });
        assert_eq!(total.load(Ordering::Relaxed), 4 * 200 * 8);
    }

    #[test]
    fn a_panic_reaches_the_caller_and_the_pool_survives() {
        let pool = SpinPool::new(3).unwrap();
        let r = catch_unwind(AssertUnwindSafe(|| {
            pool.run(12, &|p| assert!(p != 7, "part 7"));
        }));
        assert!(r.is_err());
        let total = AtomicU32::new(0);
        pool.run(12, &|_| {
            total.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(total.load(Ordering::Relaxed), 12);
    }
}
