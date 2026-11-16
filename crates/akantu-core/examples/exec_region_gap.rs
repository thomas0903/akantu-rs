// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: probe of the execution policy's thread pool (D-123).

//! Cost of a parallel region after a serial stretch.
//!
//! ```text
//! cargo run --release -p akantu-core --example exec_region_gap -- THREADS [ITEMS_5US]
//! ```
//!
//! For serial gaps of 0, 50, 200, 1000 and 2000 µs (busy work on the calling
//! thread between regions) and regions of 20 µs of serial work, prints the
//! median time of the region (µs) and of the serial reference; then, for
//! 4,000 back-to-back regions of about 5 µs of serial work, the median, the
//! 99th percentile and the maximum region time (the wait policy under
//! oversubscription). One JSON object per line. The spin time is the
//! process default (`AKANTU_EXEC_SPIN_US`).

use std::time::{Duration, Instant};

use akantu_core::exec::{Exec, ExecPolicy, Tuples};

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

fn spin_for(d: Duration) {
    let t = Instant::now();
    while t.elapsed() < d {
        std::hint::spin_loop();
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let threads: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let exec = Exec::new(
        ExecPolicy::process_default()
            .expect("policy")
            .with_threads(threads)
            .expect("threads"),
    )
    .expect("pool");
    // calibrate ~20 µs of serial work
    let mut out = vec![0.0; 4096];
    work(0, &mut out);
    let t = Instant::now();
    for r in 0..100 {
        work(std::hint::black_box(r), &mut out);
        std::hint::black_box(&out);
    }
    let per_us = 100.0 * out.len() as f64 / (t.elapsed().as_secs_f64() * 1e6);
    let items = (20.0 * per_us) as usize;
    let mut out = vec![0.0; items];
    for gap in [0u64, 50, 200, 1000, 2000] {
        let (mut par, mut ser) = (Vec::new(), Vec::new());
        for _ in 0..400 {
            spin_for(Duration::from_micros(gap));
            let t = Instant::now();
            exec.for_each(Tuples::new(&mut out, 1), 1, |first, mut o| {
                work(first, o.as_mut_slice())
            });
            par.push(t.elapsed().as_secs_f64() * 1e6);
            spin_for(Duration::from_micros(gap));
            let t = Instant::now();
            work(std::hint::black_box(0), &mut out);
            ser.push(t.elapsed().as_secs_f64() * 1e6);
        }
        std::hint::black_box(&out);
        println!(
            "{{\"threads\":{threads},\"gap_us\":{gap},\"items\":{items},\"region_us\":{:.2},\"serial_us\":{:.2}}}",
            median(&mut par),
            median(&mut ser)
        );
    }
    // distribution of ~5 µs regions back to back
    // a fixed amount of work when given (second argument), so that two
    // builds time the same regions
    let items = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(((5.0 * per_us) as usize).max(threads));
    let mut out = vec![0.0; items];
    let mut t5 = Vec::with_capacity(4000);
    for _ in 0..4000 {
        let t = Instant::now();
        exec.for_each(Tuples::new(&mut out, 1), 1, |first, mut o| {
            work(first, o.as_mut_slice())
        });
        t5.push(t.elapsed().as_secs_f64() * 1e6);
    }
    std::hint::black_box(&out);
    t5.sort_by(f64::total_cmp);
    let q = |f: f64| t5[((t5.len() - 1) as f64 * f) as usize];
    println!(
        "{{\"threads\":{threads},\"items\":{items},\"regions_5us\":{},\"p50_us\":{:.2},\"p99_us\":{:.2},\"max_us\":{:.2}}}",
        t5.len(),
        q(0.5),
        q(0.99),
        t5[t5.len() - 1]
    );
}
