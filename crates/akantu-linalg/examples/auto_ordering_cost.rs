// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs measurement tool (no Akantu counterpart): the cost of the
// orderings of the Eigen SparseLU translation when the pattern changes at
// every solve (analysis and factorisation together), for D-125's `auto`.

//! Analysis + factorisation time of COLAMD, nested dissection and `auto`
//! (a fresh analysis each time, as when the pattern changes every solve),
//! best of `R` runs, one thread, bitwise kernels, on:
//!
//! * 2-D strips and squares of `nx × ny` nodes, `c` unknowns per node,
//!   9-point connectivity (quadrilateral elements), a stiffness-like
//!   matrix (off-diagonal entries in `[-1, -0.5)`, diagonal = absolute row
//!   sum + 0.5);
//! * 3-D grids of `p³` nodes, 27-point connectivity, the same values;
//! * the indefinite 3-D grid: unit diagonal, off-diagonal entries uniform
//!   in `[-1, 1]` (it passes `auto`'s diagonal test and pivots off the
//!   diagonal).
//!
//! ```text
//! cargo run --release -p akantu-linalg --example auto_ordering_cost [-- R]
//! ```

use std::time::Instant;

use akantu_linalg::direct::CscMatrix;
use akantu_linalg::eigen_sparse_lu::{SparseLu, SparseOrdering};

fn rng(seed: u64) -> impl FnMut() -> f64 {
    let mut r = seed | 1;
    move || {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        (r >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The matrix of a structured grid: `nodes` per axis (2 or 3 axes), `c`
/// unknowns per node, every node coupled with its neighbours (8 or 26).
fn grid(nodes: &[usize], c: usize, indefinite: bool, seed: u64) -> CscMatrix {
    let mut next = rng(seed);
    let dims = nodes.len();
    let total: usize = nodes.iter().product();
    let coord = |mut id: usize| -> Vec<usize> {
        let mut x = vec![0; dims];
        for d in (0..dims).rev() {
            x[d] = id % nodes[d];
            id /= nodes[d];
        }
        x
    };
    let index = |x: &[i64]| -> Option<usize> {
        let mut id = 0usize;
        for d in 0..dims {
            if x[d] < 0 || x[d] as usize >= nodes[d] {
                return None;
            }
            id = id * nodes[d] + x[d] as usize;
        }
        Some(id)
    };
    let mut t = Vec::new();
    let mut diag = vec![0.0f64; total * c];
    let offsets: Vec<Vec<i64>> = (0..3usize.pow(dims as u32))
        .map(|k| {
            (0..dims)
                .map(|d| (k / 3usize.pow(d as u32) % 3) as i64 - 1)
                .collect()
        })
        .collect();
    for a in 0..total {
        let x = coord(a);
        for off in &offsets {
            let y: Vec<i64> = x.iter().zip(off).map(|(&p, &o)| p as i64 + o).collect();
            let Some(b) = index(&y) else { continue };
            for r in 0..c {
                for s in 0..c {
                    let (row, col) = (a * c + r, b * c + s);
                    if row != col {
                        let v = if indefinite {
                            2.0 * next() - 1.0
                        } else {
                            -0.5 - 0.5 * next()
                        };
                        t.push((row, col, v));
                        diag[row] += v.abs();
                    }
                }
            }
        }
    }
    for (i, d) in diag.iter().enumerate() {
        t.push((i, i, if indefinite { 1.0 } else { d + 0.5 }));
    }
    CscMatrix::from_triplets(total * c, &t).expect("valid triplets")
}

/// Best-of-`reps` time of a fresh analysis + factorisation; the last
/// solver (for its report).
fn time(a: &CscMatrix, o: SparseOrdering, reps: usize) -> (f64, SparseLu) {
    let mut best = f64::INFINITY;
    let mut last = SparseLu::default();
    for _ in 0..reps {
        let mut lu = SparseLu::default();
        lu.set_ordering(Some(o));
        let t = Instant::now();
        lu.analyze_pattern(a).expect("analysis");
        lu.factorize(a).expect("factorisation");
        best = best.min(t.elapsed().as_secs_f64());
        last = lu;
    }
    (best, last)
}

fn main() {
    let reps: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let mut cases: Vec<(String, CscMatrix)> = vec![
        ("strip 4 x 600, c 1".into(), grid(&[4, 600], 1, false, 1)),
        ("strip 6 x 400, c 2".into(), grid(&[6, 400], 2, false, 2)),
        ("square 30 x 30, c 3".into(), grid(&[30, 30], 3, false, 3)),
        ("square 50 x 50, c 1".into(), grid(&[50, 50], 1, false, 4)),
        ("square 50 x 50, c 3".into(), grid(&[50, 50], 3, false, 5)),
        ("indefinite 16^3".into(), grid(&[16, 16, 16], 1, true, 6)),
        ("indefinite 20^3".into(), grid(&[20, 20, 20], 1, true, 7)),
    ];
    for &(n, c) in &[
        (70, 1),
        (100, 1),
        (140, 1),
        (200, 1),
        (70, 2),
        (100, 2),
        (70, 3),
        (100, 3),
    ] {
        cases.push((
            format!("square {n} x {n}, c {c}"),
            grid(&[n, n], c, false, 8),
        ));
    }
    for &(p, c) in &[
        (12, 1),
        (15, 1),
        (18, 1),
        (22, 1),
        (26, 1),
        (8, 3),
        (10, 3),
        (12, 3),
        (14, 3),
    ] {
        cases.push((format!("cube {p}^3, c {c}"), grid(&[p, p, p], c, false, 9)));
    }
    println!(
        "| system | n | COLAMD predicted work | COLAMD | ND / COLAMD | auto / COLAMD | auto chose | fallback |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for (name, a) in &cases {
        let (tc, _) = time(a, SparseOrdering::Colamd, reps);
        let (tn, _) = time(a, SparseOrdering::NestedDissection, reps);
        let (ta, lu) = time(a, SparseOrdering::Auto, reps);
        let r = lu.ordering_report();
        let work = r
            .predicted_colamd
            .map_or("–".into(), |(_, w)| format!("{w:.2e}"));
        println!(
            "| {name} | {} | {work} | {:.4} s | {:.3} | {:.3} | {} | {} |",
            a.n(),
            tc,
            tn / tc,
            ta / tc,
            r.used,
            r.fallback
        );
    }
}
