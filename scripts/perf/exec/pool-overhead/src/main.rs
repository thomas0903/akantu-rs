// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs project tooling (not ported from Akantu): the cost of one
// parallel region for the pool choice of D-123.
//
//   pool-overhead [--threads 2,4,8] [--work-us 0,10,30,100] [--reps 2000] [--rounds 5]
//
// For every thread count and amount of work (the serial time of the region's
// loop, calibrated on this machine), the median time of one region with:
//   serial   the loop on the calling thread (the reference, threads = 1)
//   exec     `akantu_core::exec::Exec::for_each` (rayon pool, 4 parts per thread)
//   rayon-b  `rayon::ThreadPool::broadcast` (one call per thread, static split)
//   spin     a hand-written pool: workers spin on a generation counter (then
//            park), the caller takes part 0 (static split)
// Overhead = region time − serial time / threads. One JSON object per line.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use akantu_core::exec::{Exec, ExecPolicy, Tuples};

/// Independent per-item work (a short dependent chain, no memory traffic
/// beyond the output).
#[inline(never)]
fn work(first: usize, out: &mut [f64]) {
    for (k, o) in out.iter_mut().enumerate() {
        let mut x = (first + k) as f64 * 1e-3 + 1.0;
        for _ in 0..8 {
            x = x * 0.999_999 + 1e-7;
        }
        *o = x;
    }
}

/// A pool whose workers spin on a generation counter (up to `SPIN` polls,
/// then wait on a condition variable); part `p` of a region runs on thread
/// `p` (the caller is thread 0).
struct SpinPool {
    shared: Arc<Shared>,
    threads: usize,
}

struct Shared {
    generation: AtomicU64,
    done: AtomicUsize,
    /// The current job, a `&(dyn Fn(usize) + Sync)` with its lifetime erased.
    job: Mutex<Option<JobPtr>>,
    wake: Condvar,
    stop: AtomicBool,
}

#[derive(Clone, Copy)]
struct JobPtr(*const (dyn Fn(usize) + Sync));
// SAFETY: the pointee is `Sync`, and `run` keeps it alive until every worker
// reported the end of its part.
unsafe impl Send for JobPtr {}

const SPIN: u32 = 200_000;

impl SpinPool {
    fn new(threads: usize) -> Self {
        let shared = Arc::new(Shared {
            generation: AtomicU64::new(0),
            done: AtomicUsize::new(0),
            job: Mutex::new(None),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        for t in 1..threads {
            let s = shared.clone();
            std::thread::spawn(move || {
                let mut seen = 0u64;
                loop {
                    let mut spins = 0;
                    let g = loop {
                        let g = s.generation.load(Ordering::Acquire);
                        if g != seen || s.stop.load(Ordering::Relaxed) {
                            break g;
                        }
                        spins += 1;
                        if spins < SPIN {
                            std::hint::spin_loop();
                        } else {
                            let guard = s.job.lock().unwrap();
                            if s.generation.load(Ordering::Acquire) == seen {
                                let _g = s.wake.wait(guard).unwrap();
                            }
                            spins = 0;
                        }
                    };
                    if s.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    seen = g;
                    let job = s.job.lock().unwrap().expect("job set before the generation");
                    // SAFETY: `run` published the pointer before bumping the
                    // generation and waits for `done` before returning, so
                    // the closure outlives this call.
                    unsafe { (*job.0)(t) };
                    s.done.fetch_add(1, Ordering::AcqRel);
                }
            });
        }
        Self { shared, threads }
    }

    fn run(&self, f: &(dyn Fn(usize) + Sync)) {
        let s = &self.shared;
        s.done.store(0, Ordering::Relaxed);
        {
            let mut job = s.job.lock().unwrap();
            // SAFETY (lifetime erasure): the pointer is used only until every
            // worker incremented `done`, which this function waits for.
            let erased: *const (dyn Fn(usize) + Sync) = unsafe { std::mem::transmute(f as *const (dyn Fn(usize) + Sync)) };
            *job = Some(JobPtr(erased));
            s.generation.fetch_add(1, Ordering::AcqRel);
        }
        s.wake.notify_all();
        f(0);
        while s.done.load(Ordering::Acquire) != self.threads - 1 {
            std::hint::spin_loop();
        }
    }
}

impl Drop for SpinPool {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.wake.notify_all();
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn list(v: Option<String>, default: &[usize]) -> Vec<usize> {
    v.map_or_else(|| default.to_vec(), |s| s.split(',').map(|x| x.parse().expect("number")).collect())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut threads, mut works, mut reps, mut rounds) = (vec![2, 4, 8], vec![0, 10, 30, 100], 2000, 5);
    while let Some(k) = args.next() {
        match k.as_str() {
            "--threads" => threads = list(args.next(), &threads),
            "--work-us" => works = list(args.next(), &works),
            "--reps" => reps = args.next().expect("value").parse().expect("number"),
            "--rounds" => rounds = args.next().expect("value").parse().expect("number"),
            _ => panic!("unknown argument {k}"),
        }
    }
    // calibrate: items per microsecond of serial work
    let mut buf = vec![0.0; 1 << 16];
    work(0, &mut buf); // first touch
    let t = Instant::now();
    for _ in 0..200 {
        work(0, &mut buf);
    }
    let per_us = (200.0 * buf.len() as f64 / (t.elapsed().as_secs_f64() * 1e6)).max(1.0);
    for &n in &threads {
        let exec = Exec::new(ExecPolicy::serial().with_threads(n).unwrap()).unwrap();
        let rayon_pool = rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap();
        let spin = SpinPool::new(n);
        for &w in &works {
            let items = ((w as f64 * per_us) as usize).max(n);
            let mut out = vec![0.0; items];
            for round in 0..rounds {
                let mut variants: Vec<&str> = vec!["serial", "exec", "rayon-b", "spin"];
                variants.rotate_left(round % 4);
                for v in variants {
                    let mut times = Vec::with_capacity(reps);
                    for _ in 0..reps {
                        let t = Instant::now();
                        match v {
                            "serial" => work(0, &mut out),
                            "exec" => exec.for_each(Tuples::new(&mut out, 1), 1, |first, mut o| work(first, o.as_mut_slice())),
                            "rayon-b" => {
                                let ptr = SyncPtr(out.as_mut_ptr());
                                rayon_pool.broadcast(|ctx| {
                                    let (i, m) = (ctx.index(), ctx.num_threads());
                                    let (a, b) = (i * items / m, (i + 1) * items / m);
                                    let p = ptr;
                                    // SAFETY: the parts [a, b) of the threads are disjoint
                                    // and inside `out`, which outlives the broadcast.
                                    let part = unsafe { std::slice::from_raw_parts_mut(p.0.add(a), b - a) };
                                    work(a, part);
                                });
                            }
                            _ => {
                                let ptr = SyncPtr(out.as_mut_ptr());
                                spin.run(&|i| {
                                    let (a, b) = (i * items / n, (i + 1) * items / n);
                                    let p = ptr;
                                    // SAFETY: as above.
                                    let part = unsafe { std::slice::from_raw_parts_mut(p.0.add(a), b - a) };
                                    work(a, part);
                                });
                            }
                        }
                        times.push(t.elapsed().as_secs_f64() * 1e6);
                    }
                    std::hint::black_box(&out);
                    let med = median(&mut times);
                    println!(
                        "{{\"threads\":{n},\"work_us\":{w},\"items\":{items},\"round\":{round},\"variant\":\"{v}\",\"median_us\":{med:.3},\"min_us\":{:.3}}}",
                        times[0]
                    );
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SyncPtr(*mut f64);
// SAFETY: used only to hand disjoint parts of one slice to the threads of a
// region that ends before the slice is used again.
unsafe impl Send for SyncPtr {}
// SAFETY: as above.
unsafe impl Sync for SyncPtr {}
