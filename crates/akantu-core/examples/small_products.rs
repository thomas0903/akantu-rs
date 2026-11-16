// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs project tooling (not ported from Akantu): micro-benchmark of the
// fixed-order slice kernels (`eigen_kernels::SliceProduct`), the general
// closure route (`eigen_gebp::product_into`) and the order-free fast-mode
// variants (`eigen_kernels::fast_mode`) on the hottest FE product classes.
// Numbers from it are not evidence for the default path (the benchmark
// suite's cases are); they size the fast-mode gain.
//
//   cargo run --release -p akantu-core --example small_products [REPS]

use std::hint::black_box;
use std::time::Instant;

use akantu_core::eigen_gebp::{Extent, ProductShape, product_into};
use akantu_core::eigen_kernels::{SliceProduct, fast_mode};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn time(reps: usize, iters: usize, mut f: impl FnMut()) -> f64 {
    let mut runs = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        for _ in 0..iters {
            f();
        }
        runs.push(t.elapsed().as_secs_f64() * 1e9 / iters as f64);
    }
    median(runs)
}

fn main() {
    let reps: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    // (name, shape, lhs stored transposed for gemm_tn)
    let classes: [(&str, ProductShape, bool); 7] = [
        (
            "BtD 30x6x6 (tet10 stiffness)",
            ProductShape::dynamic(30, 6, 6).lhs_transposed(),
            true,
        ),
        ("BtDB 30x6x30", ProductShape::dynamic(30, 6, 30), false),
        (
            "NtN 30x3x30 (tet10 mass)",
            ProductShape::dynamic(30, 3, 30),
            false,
        ),
        (
            "BtDB 18x3x18 (DKT18)",
            ProductShape::dynamic(18, 3, 18),
            false,
        ),
        (
            "J 3x10x3 (tet10 Jacobian, tree)",
            ProductShape::new(
                Extent::Fixed(3),
                Extent::Fixed(10),
                Extent::Dynamic(10),
                Extent::Dynamic(3),
            ),
            false,
        ),
        (
            "C eps 6x6x1 (anisotropic)",
            ProductShape::new(
                Extent::Dynamic(6),
                Extent::Dynamic(6),
                Extent::Fixed(6),
                Extent::Fixed(1),
            ),
            false,
        ),
        (
            "D grad 3x3x1 (heat)",
            ProductShape::new(
                Extent::Dynamic(3),
                Extent::Dynamic(3),
                Extent::Fixed(3),
                Extent::Fixed(1),
            ),
            false,
        ),
    ];
    println!(
        "| class | route | general (ns) | slice kernel (ns) | fast mode (ns) | fast / slice |"
    );
    println!("|---|---|---|---|---|---|");
    for (name, shape, tn) in classes {
        let (m, k, n) = (shape.rows.size(), shape.lhs_depth.size(), shape.cols.size());
        let a: Vec<f64> = (0..m * k)
            .map(|i| ((i * 7919) % 101) as f64 * 0.013 - 0.6)
            .collect();
        let b: Vec<f64> = (0..k * n)
            .map(|i| ((i * 104729) % 97) as f64 * 0.011 - 0.5)
            .collect();
        let mut out = vec![0.0; m * n];
        let p = SliceProduct::new(shape).unwrap();
        let iters = (2_000_000 / (m * k * n)).max(1000);
        let (ai, al) = if tn { (k, 1) } else { (1, m) };
        let general = time(reps, iters, || {
            product_into(
                &shape,
                &|i, l| black_box(&a)[i * ai + l * al],
                &|l, j| black_box(&b)[l + j * k],
                &mut out,
            )
            .unwrap();
            black_box(&out);
        });
        let slice = time(reps, iters, || {
            p.eval(black_box(&a), black_box(&b), &mut out).unwrap();
            black_box(&out);
        });
        let fast = time(reps, iters, || {
            if tn {
                fast_mode::gemm_tn(m, k, n, black_box(&a), black_box(&b), &mut out);
            } else {
                fast_mode::gemm_colmajor(m, k, n, black_box(&a), black_box(&b), &mut out);
            }
            black_box(&out);
        });
        println!(
            "| {name} | {:?} | {general:.1} | {slice:.1} | {fast:.1} | {:.2} |",
            p.route(),
            fast / slice
        );
    }
}
