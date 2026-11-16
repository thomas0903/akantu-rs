// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the thread pool of the execution policy (D-123).

//! A fixed pool of worker threads for short parallel regions.
//!
//! One region at a time: the caller publishes a job (`Fn(part)`) and the
//! number of parts, wakes the workers, and takes parts itself. Participant
//! `i` first takes the parts of the `i`-th contiguous block, then the
//! leftovers of the others, so a thread keeps its elements from one region
//! to the next. A worker joins a region only by entering it while it is
//! open; when the caller has no part left it closes the region and waits
//! only for the workers that entered, so a worker that wakes late (parked
//! during a serial stretch) skips the region instead of making the caller
//! wait for its wake-up. Between regions a worker spins for the policy's
//! spin time (default 5 ms: the serial stretches of an explicit step last
//! about a millisecond; 0 when the threads outnumber the cores), then parks
//! on a condition variable. The caller's wait for the workers inside a
//! region yields the core after a bounded number of polls.
//!
//! No unwind can leave a region open: the caller closes the region and waits
//! for the workers inside in a guard's `Drop`, a worker leaves the region in
//! a guard's `Drop`, a part's panic is caught inside the region, and its
//! payload is dropped only after the region is closed and empty (the first
//! one is resumed on the caller, the others are dropped there, a panic of
//! such a `Drop` caught and its payload forgotten). Measured cost of one region on 8 cores of
//! an AMD EPYC 9R14: 0.2–1.5 µs at 2–8 threads (3.7 µs at most), against
//! 5–14 µs for a `rayon` pool
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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Polls of the caller's wait for the workers inside a region before it
/// starts yielding the core (a worker descheduled inside the region would
/// otherwise hold the caller for a whole scheduler slice of busy waiting).
const POLLS_BEFORE_YIELD: u32 = 2048;

/// Parts per thread for which the claim flags exist (regions with more parts
/// claim them with a counter).
const MAX_PARTS_PER_THREAD: usize = 8;

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
    /// Next unclaimed part of the running region (regions with more parts
    /// than `claimed` holds).
    next: AtomicUsize,
    /// Per part of the running region: claimed by a thread.
    claimed: Box<[AtomicBool]>,
    /// Participants of a region: the caller (0) and the workers (1…).
    participants: usize,
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
    /// The payloads of the other panics of the running region, dropped by
    /// its owner once the region is closed and empty (a payload's `Drop`
    /// may panic too).
    discarded: Mutex<Vec<Box<dyn Any + Send>>>,
    /// How long an idle worker spins before parking (ns), set by the owner
    /// of each region from its policy.
    spin_ns: AtomicU64,
    /// Set when the pool's creation failed: the workers already started
    /// leave.
    stop: AtomicBool,
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
            claimed: (0..threads * MAX_PARTS_PER_THREAD)
                .map(|_| AtomicBool::new(false))
                .collect(),
            participants: threads.max(1),
            nb_parts: AtomicUsize::new(0),
            state: AtomicU64::new(CLOSED),
            sleepers: AtomicUsize::new(0),
            job: UnsafeCell::new(None),
            panic: Mutex::new(None),
            discarded: Mutex::new(Vec::new()),
            spin_ns: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            sleep: Mutex::new(()),
            wake: Condvar::new(),
        });
        let workers = threads.saturating_sub(1);
        let mut started = Vec::with_capacity(workers);
        for w in 0..workers {
            let s = shared.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("akantu-exec-{threads}-{w}"))
                .spawn(move || worker(&s, w + 1));
            match spawned {
                Ok(h) => started.push(h),
                Err(e) => {
                    // the workers already started leave and are joined
                    shared.stop.store(true, Ordering::SeqCst);
                    shared.generation.fetch_add(1, Ordering::SeqCst);
                    {
                        let _g = shared.sleep.lock().unwrap_or_else(|e| e.into_inner());
                        shared.wake.notify_all();
                    }
                    for h in started {
                        let _ = h.join();
                    }
                    return Err(e);
                }
            }
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
    /// resumed on the caller after the region ended. `spin`: how long the
    /// workers spin after this region before they park.
    pub(crate) fn run(&self, nb_parts: usize, spin: Duration, f: &(dyn Fn(usize) + Sync)) {
        let serial = || (0..nb_parts).for_each(f);
        if self.workers == 0 || nb_parts <= 1 || IN_REGION.with(Cell::get) {
            return serial();
        }
        // a lock poisoned by a part's panic resumed on an earlier caller is
        // taken as it is: that region was closed and emptied by its guard
        let _busy = match self.busy.try_lock() {
            Ok(g) => g,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return serial(),
        };
        let s = &*self.shared;
        // SAFETY: lifetime erasure of `f`; the pointer is published to the
        // workers below, and `region` (created before the region opens)
        // closes the region, waits until no worker is inside and clears the
        // slot in its `Drop`, which runs before this function returns or
        // unwinds, so no worker dereferences it after `f`'s borrow ends.
        let erased: *const (dyn Fn(usize) + Sync) =
            unsafe { std::mem::transmute(f as *const (dyn Fn(usize) + Sync)) };
        // SAFETY: we hold `busy` and the previous region is closed with no
        // worker inside (its guard's `Drop` ran), so no worker reads `job`.
        unsafe { *s.job.get() = Some(JobPtr(erased)) };
        let region = RegionGuard(s);
        s.next.store(0, Ordering::Relaxed);
        for c in s.claimed.iter().take(nb_parts) {
            c.store(false, Ordering::Relaxed);
        }
        s.nb_parts.store(nb_parts, Ordering::Relaxed);
        s.spin_ns.store(
            u64::try_from(spin.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        let g = s.generation.load(Ordering::Relaxed).wrapping_add(1);
        // open the region, then announce it
        s.state.store((g & GEN_MASK) << 16, Ordering::SeqCst);
        s.generation.store(g, Ordering::SeqCst);
        if s.sleepers.load(Ordering::SeqCst) > 0 {
            let _g = s.sleep.lock().unwrap_or_else(|e| e.into_inner());
            s.wake.notify_all();
        }
        run_parts(s, 0, f);
        // no part left: close the region, wait for the workers inside
        drop(region);
        // the region is closed and empty: drop the other payloads here
        let discarded = std::mem::take(&mut *s.discarded.lock().unwrap_or_else(|e| e.into_inner()));
        for payload in discarded {
            if let Err(e) = catch_unwind(AssertUnwindSafe(move || drop(payload))) {
                std::mem::forget(e);
            }
        }
        let panic = s.panic.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(p) = panic {
            resume_unwind(p);
        }
    }
}

/// Closes the running region and waits until no worker is inside, then
/// clears the job, when dropped: on the normal path and on every unwind out
/// of [`SpinPool::run`].
struct RegionGuard<'a>(&'a Shared);

impl Drop for RegionGuard<'_> {
    fn drop(&mut self) {
        let s = self.0;
        s.state.fetch_or(CLOSED, Ordering::SeqCst);
        let mut polls = 0u32;
        while s.state.load(Ordering::Acquire) & ACTIVE != 0 {
            if polls < POLLS_BEFORE_YIELD {
                polls += 1;
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
        }
        // SAFETY: the region is closed and no worker is inside: none reads
        // `job` until the next region opens.
        unsafe { *s.job.get() = None };
    }
}

/// A worker's presence in a region: leaves it (decrements the count of
/// workers inside) when dropped, also during an unwind.
struct Inside<'a>(&'a Shared);

impl Drop for Inside<'_> {
    fn drop(&mut self) {
        self.0.state.fetch_sub(1, Ordering::Release);
    }
}

/// Clears the thread's in-region flag when dropped.
struct InRegion;

impl InRegion {
    fn enter() -> Self {
        IN_REGION.with(|r| r.set(true));
        Self
    }
}

impl Drop for InRegion {
    fn drop(&mut self) {
        IN_REGION.with(|r| r.set(false));
    }
}

/// Claim and run parts of the current region until none is left.
/// Participant `index` first takes the parts of its own block (the
/// `index`-th of `participants` contiguous blocks), then those left in the
/// other blocks: a thread keeps its elements from one region to the next,
/// so their data stays in its caches.
fn run_parts(s: &Shared, index: usize, f: &(dyn Fn(usize) + Sync)) {
    let n = s.nb_parts.load(Ordering::Relaxed);
    let _in_region = InRegion::enter();
    let run = |p: usize| {
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| f(p))) {
            // never drop a payload inside the region (its `Drop` may panic)
            let mut slot = s.panic.lock().unwrap_or_else(|e| e.into_inner());
            if slot.is_none() {
                *slot = Some(e);
            } else {
                drop(slot);
                s.discarded
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(e);
            }
        }
    };
    if n <= s.claimed.len() {
        let t = s.participants;
        for k in 0..t {
            let block = (index + k) % t;
            for p in block * n / t..(block + 1) * n / t {
                // a load first: the parts of a finished block are skipped
                // without writing their cache line
                if !s.claimed[p].load(Ordering::Relaxed)
                    && !s.claimed[p].swap(true, Ordering::Relaxed)
                {
                    run(p);
                }
            }
        }
    } else {
        loop {
            let p = s.next.fetch_add(1, Ordering::Relaxed);
            if p >= n {
                break;
            }
            run(p);
        }
    }
}

fn worker(s: &Shared, index: usize) {
    // the generation at the pool's creation: a region published before
    // this thread started is not missed
    let mut seen = 0;
    loop {
        // wait for the next region: spin for the last region's spin time,
        // then park
        let start = Instant::now();
        let spin = Duration::from_nanos(s.spin_ns.load(Ordering::Relaxed));
        let mut polls = 0u32;
        loop {
            if s.generation.load(Ordering::SeqCst) != seen {
                break;
            }
            polls = polls.wrapping_add(1);
            if !spin.is_zero() && (!polls.is_multiple_of(64) || start.elapsed() < spin) {
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
        if s.stop.load(Ordering::SeqCst) {
            return;
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
        // leaves the region when dropped, also if `run_parts` unwinds
        let _inside = Inside(s);
        // SAFETY: this worker is inside the open region (counted in `state`
        // until `_inside` drops): its owner neither clears nor rewrites
        // `job` before every worker inside has left, so `job` holds the
        // region's live closure.
        let job = unsafe { *s.job.get() };
        if let Some(JobPtr(f)) = job {
            // SAFETY: as above: the closure outlives the region.
            run_parts(s, index, unsafe { &*f });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    const SPIN: Duration = Duration::from_millis(5);

    /// Repetitions: fewer under Miri (an interpreter).
    fn reps(n: usize) -> usize {
        if cfg!(miri) { n.div_ceil(10).max(2) } else { n }
    }

    /// A panic payload whose `Drop` panics.
    struct Bomb;

    impl Drop for Bomb {
        fn drop(&mut self) {
            if !std::thread::panicking() {
                panic!("bomb");
            }
        }
    }

    /// Panics with a [`Bomb`] in every part: payloads beyond the first are
    /// dropped by the caller after the region (their `Drop` panics there),
    /// the first is resumed on the caller. After `run` unwinds no part runs
    /// any more (the writes after a delay land before `run` returns), and
    /// the pool works afterwards.
    #[test]
    fn panicking_payload_drops_never_leave_a_region_open() {
        for (threads, spin) in [(2, SPIN), (4, SPIN), (8, Duration::ZERO)] {
            let pool = SpinPool::new(threads).unwrap();
            for _ in 0..reps(20) {
                let writes = AtomicU32::new(0);
                let r = catch_unwind(AssertUnwindSafe(|| {
                    pool.run(16, spin, &|_| {
                        std::thread::sleep(Duration::from_micros(200));
                        writes.fetch_add(1, Ordering::SeqCst);
                        std::panic::panic_any(Bomb);
                    });
                }));
                let after = writes.load(Ordering::SeqCst);
                assert_eq!(after, 16, "every part ran before run returned");
                std::thread::sleep(Duration::from_millis(2));
                assert_eq!(
                    writes.load(Ordering::SeqCst),
                    after,
                    "a part ran after run returned"
                );
                // the resumed payload is a Bomb: dropping it panics, caught
                let payload = r.expect_err("the parts panicked");
                assert!(payload.is::<Bomb>());
                let d = catch_unwind(AssertUnwindSafe(move || drop(payload)));
                if let Err(e) = d {
                    std::mem::forget(e);
                }
            }
            let total = AtomicU32::new(0);
            pool.run(32, spin, &|_| {
                total.fetch_add(1, Ordering::Relaxed);
            });
            assert_eq!(total.load(Ordering::Relaxed), 32);
        }
    }

    /// A spin time of zero parks the workers at once; regions still run on
    /// several threads after the wake-up.
    #[test]
    fn zero_spin_parks_and_still_runs_every_part() {
        let pool = SpinPool::new(4).unwrap();
        for _ in 0..reps(50) {
            let hits: Vec<AtomicU32> = (0..24).map(|_| AtomicU32::new(0)).collect();
            pool.run(24, Duration::ZERO, &|p| {
                hits[p].fetch_add(1, Ordering::Relaxed);
            });
            assert!(hits.iter().all(|h| h.load(Ordering::Relaxed) == 1));
        }
    }

    #[test]
    fn every_part_runs_once() {
        for threads in [1, 2, 3, 8] {
            let pool = SpinPool::new(threads).unwrap();
            for n in [0, 1, 2, 7, 100] {
                let hits: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
                pool.run(n, SPIN, &|p| {
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
            pool.run(16, SPIN, &|_| {
                // a nested region runs serially
                pool.run(3, SPIN, &|_| {
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
                    for _ in 0..reps(200) {
                        pool.run(8, SPIN, &|_| {
                            total.fetch_add(1, Ordering::Relaxed);
                        });
                    }
                });
            }
        });
        assert_eq!(total.load(Ordering::Relaxed), 4 * reps(200) as u32 * 8);
    }

    #[test]
    fn a_panic_reaches_the_caller_and_the_pool_survives() {
        let pool = SpinPool::new(3).unwrap();
        let r = catch_unwind(AssertUnwindSafe(|| {
            pool.run(12, SPIN, &|p| assert!(p != 7, "part 7"));
        }));
        assert!(r.is_err());
        let total = AtomicU32::new(0);
        pool.run(12, SPIN, &|_| {
            total.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(total.load(Ordering::Relaxed), 12);
    }
}
