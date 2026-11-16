// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs project tooling (not ported from Akantu): micro-benchmark that
// sizes a fast-mode candidate, closed-form eigenvalues of symmetric 3 × 3
// matrices (the trigonometric solution of the characteristic cubic), against
// the bitwise `eig3_fixed` (Eigen's EigenSolver order) on strain-like inputs.
// Not called by the library: the closed form changes the last bits of the
// eigenvalues and therefore belongs to the fast mode (D-122).
//
//   cargo run --release -p akantu-linalg --example eig3_closed_form [N] [REPS]

use std::hint::black_box;
use std::time::Instant;

use akantu_linalg::dense::eig3_fixed;

/// Deterministic xorshift64* draws in `[-1, 1)`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let m = x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11;
        (m as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// Eigenvalues of the symmetric matrix `a` (column-major, lower triangle
/// read) by the trigonometric solution of the characteristic cubic,
/// ascending.
fn eig3_sym_closed_form(a: &[[f64; 3]; 3]) -> [f64; 3] {
    let (a00, a11, a22) = (a[0][0], a[1][1], a[2][2]);
    let (a10, a20, a21) = (a[0][1], a[0][2], a[1][2]);
    let p1 = a10 * a10 + a20 * a20 + a21 * a21;
    if p1 == 0.0 {
        let mut d = [a00, a11, a22];
        d.sort_by(f64::total_cmp);
        return d;
    }
    let q = (a00 + a11 + a22) / 3.0;
    let (b00, b11, b22) = (a00 - q, a11 - q, a22 - q);
    let p2 = b00 * b00 + b11 * b11 + b22 * b22 + 2.0 * p1;
    let p = (p2 / 6.0).sqrt();
    let inv = 1.0 / p;
    let (c00, c11, c22) = (b00 * inv, b11 * inv, b22 * inv);
    let (c10, c20, c21) = (a10 * inv, a20 * inv, a21 * inv);
    let det = c00 * (c11 * c22 - c21 * c21) - c10 * (c10 * c22 - c21 * c20)
        + c20 * (c10 * c21 - c11 * c20);
    let r = (det / 2.0).clamp(-1.0, 1.0);
    let phi = r.acos() / 3.0;
    let e_max = q + 2.0 * p * phi.cos();
    let e_min = q + 2.0 * p * (phi + 2.0 * std::f64::consts::FRAC_PI_3).cos();
    [e_min, 3.0 * q - e_max - e_min, e_max]
}

#[allow(clippy::needless_range_loop)] // the symmetric fill writes a[j][i] and a[i][j]
fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    let n = args.first().copied().unwrap_or(200_000);
    let reps = args.get(1).copied().unwrap_or(9);
    // strain-like inputs: entries of order 1e-4, a fifth with a zero
    // off-diagonal pair, a tenth with a repeated diagonal
    let mut rng = Rng(0x0e16_3c0f);
    let mats: Vec<[[f64; 3]; 3]> = (0..n)
        .map(|k| {
            let mut a = [[0.0; 3]; 3];
            for j in 0..3 {
                for i in j..3 {
                    let x = 1e-4 * rng.next();
                    a[j][i] = x;
                    a[i][j] = x;
                }
            }
            if k % 5 == 0 {
                a[1][0] = 0.0;
                a[0][1] = 0.0;
            }
            if k % 10 == 0 {
                a[1][1] = a[0][0];
            }
            a
        })
        .collect();

    // accuracy of the closed form against the bitwise path
    let mut max_rel = 0.0_f64;
    let mut differing = 0usize;
    for a in &mats {
        let mut s = eig3_fixed(a).expect("converges on strain-like input");
        s.sort_by(f64::total_cmp);
        let c = eig3_sym_closed_form(a);
        let scale = s.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        for (x, y) in s.iter().zip(&c) {
            if x.to_bits() != y.to_bits() {
                differing += 1;
            }
            if scale > 0.0 {
                max_rel = max_rel.max((x - y).abs() / scale);
            }
        }
    }

    // timing: alternate the two variants, median ns per matrix
    let mut t_schur = Vec::with_capacity(reps);
    let mut t_closed = Vec::with_capacity(reps);
    for r in 0..reps {
        for variant in [r % 2, 1 - r % 2] {
            let t0 = Instant::now();
            let mut acc = 0.0;
            for a in &mats {
                let v = if variant == 0 {
                    eig3_fixed(black_box(a)).unwrap_or([0.0; 3])
                } else {
                    eig3_sym_closed_form(black_box(a))
                };
                acc += v[0] + v[1] + v[2];
            }
            black_box(acc);
            let ns = t0.elapsed().as_secs_f64() * 1e9 / n as f64;
            if variant == 0 {
                t_schur.push(ns);
            } else {
                t_closed.push(ns);
            }
        }
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let (ms, mc) = (median(&mut t_schur), median(&mut t_closed));
    println!("matrices {n}, repetitions {reps}");
    println!("eig3_fixed (bitwise, Eigen order): {ms:.1} ns per matrix");
    println!("closed form (fast mode):           {mc:.1} ns per matrix");
    println!("ratio closed / bitwise:            {:.3}", mc / ms);
    println!(
        "eigenvalues differing in bits: {differing} of {}; max |diff| / max|lambda|: {max_rel:.3e}",
        3 * n
    );
}
