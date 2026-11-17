// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs measurement tool (no Akantu counterpart): the fill and the
// operation count of the Eigen SparseLU translation with a geometric
// nested-dissection column order instead of COLAMD.

//! Fill of COLAMD against a geometric nested dissection on the 3-D grid
//! matrices of the benchmark suites' kind (hexahedral connectivity: every
//! node coupled with its 26 neighbours, `c` unknowns per node: 3 for
//! elasticity, 1 for heat). For each ordering: the nonzeros of `L` and
//! `U`, the multiply-adds of the factorisation counted from the factors'
//! structure (`Σ_j (|L(:,j)| − 1)·(|U(j,:)| − 1)`), and the factorisation
//! time (best of three, one thread, bitwise kernels).
//!
//! ```text
//! cargo run --release -p akantu-linalg --example nested_dissection_fill -- N C
//! ```
//! (`N` cells per side: `(N + 1)³` nodes; `C` unknowns per node.)

use std::time::Instant;

use akantu_linalg::direct::CscMatrix;
use akantu_linalg::eigen_sparse_lu::{SparseLu, SparseLuFactors};

/// The grid matrix: diagonally dominant, slightly unsymmetric values.
fn grid_matrix(n: usize, c: usize) -> CscMatrix {
    let p = n + 1;
    let node = |i: usize, j: usize, k: usize| (i * p + j) * p + k;
    let mut t = Vec::new();
    for i in 0..p {
        for j in 0..p {
            for k in 0..p {
                let a = node(i, j, k);
                for di in -1i64..=1 {
                    for dj in -1i64..=1 {
                        for dk in -1i64..=1 {
                            let (x, y, z) = (i as i64 + di, j as i64 + dj, k as i64 + dk);
                            if x < 0 || y < 0 || z < 0 {
                                continue;
                            }
                            let (x, y, z) = (x as usize, y as usize, z as usize);
                            if x >= p || y >= p || z >= p {
                                continue;
                            }
                            let b = node(x, y, z);
                            for r in 0..c {
                                for s in 0..c {
                                    let v = if a == b && r == s {
                                        30.0 * c as f64
                                    } else {
                                        -1.0 + 0.01 * ((a * 7 + b * 3 + r + 2 * s) % 11) as f64
                                    };
                                    t.push((a * c + r, b * c + s, v));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    CscMatrix::from_triplets(p * p * p * c, &t).expect("valid triplets")
}

/// Geometric nested dissection of the nodes in the box `lo..hi` (per
/// axis): the two halves of the longest axis, then the separating plane.
fn dissect(lo: [usize; 3], hi: [usize; 3], p: usize, out: &mut Vec<usize>) {
    let len = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let node = |i: usize, j: usize, k: usize| (i * p + j) * p + k;
    if len.contains(&0) {
        return;
    }
    if len.iter().all(|&l| l <= 3) {
        for i in lo[0]..hi[0] {
            for j in lo[1]..hi[1] {
                for k in lo[2]..hi[2] {
                    out.push(node(i, j, k));
                }
            }
        }
        return;
    }
    let ax = (0..3).max_by_key(|&a| len[a]).unwrap_or(0);
    let mid = lo[ax] + len[ax] / 2;
    let (mut h1, mut l2) = (hi, lo);
    h1[ax] = mid;
    l2[ax] = mid + 1;
    dissect(lo, h1, p, out);
    dissect(l2, hi, p, out);
    let (mut ls, mut hs) = (lo, hi);
    ls[ax] = mid;
    hs[ax] = mid + 1;
    for i in ls[0]..hs[0] {
        for j in ls[1]..hs[1] {
            for k in ls[2]..hs[2] {
                out.push(node(i, j, k));
            }
        }
    }
}

/// `(nnz(L) + nnz(U), multiply-adds)` from the factor structure.
fn counts(n: usize, f: &SparseLuFactors) -> (i64, f64) {
    // |L(:,j)| (with the diagonal) and |U(j,:)| (with the diagonal)
    let mut lcol = vec![0i64; n];
    let mut urow = vec![0i64; n];
    let nsuper = f.xsup.len() - 1;
    for s in 0..nsuper {
        let (f0, f1) = (f.xsup[s] as usize, f.xsup[s + 1] as usize);
        if f0 >= n {
            break;
        }
        let rows = f.xlsub[f0 + 1] - f.xlsub[f0];
        for j in f0..f1.min(n) {
            lcol[j] = rows - (j - f0) as i64;
            // the supernode's upper triangle: row j has columns j..f1
            urow[j] += (f1.min(n) - j) as i64;
        }
    }
    for &r in &f.usub {
        urow[r as usize] += 1;
    }
    let flops = (0..n)
        .map(|j| (lcol[j] - 1).max(0) as f64 * (urow[j] - 1).max(0) as f64)
        .sum();
    (f.nnz_l + f.nnz_u, flops)
}

fn measure(a: &CscMatrix, order: Option<&[usize]>) -> (i64, f64, f64) {
    let mut lu = SparseLu::default();
    match order {
        Some(o) => lu.analyze_pattern_with_order(a, o).expect("analysis"),
        None => lu.analyze_pattern(a).expect("analysis"),
    }
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let t = Instant::now();
        lu.factorize(a).expect("factorisation");
        best = best.min(t.elapsed().as_secs_f64());
    }
    let (nnz, flops) = counts(a.n(), &lu.factors().expect("factors"));
    (nnz, flops, best)
}

fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("N C"))
        .collect();
    let (n, c) = (
        args.first().copied().unwrap_or(14),
        args.get(1).copied().unwrap_or(3),
    );
    let a = grid_matrix(n, c);
    let p = n + 1;
    let mut nodes = Vec::with_capacity(p * p * p);
    dissect([0; 3], [p; 3], p, &mut nodes);
    let order: Vec<usize> = nodes
        .iter()
        .flat_map(|&v| (0..c).map(move |r| v * c + r))
        .collect();
    let (n0, f0, t0) = measure(&a, None);
    let (n1, f1, t1) = measure(&a, Some(&order));
    println!(
        "grid {n}^3 cells, {c} per node, {} unknowns, {} nonzeros",
        a.n(),
        a.nnz()
    );
    println!("COLAMD           nnz(L+U) {n0:>12}  multiply-adds {f0:>14.4e}  factorise {t0:.4} s");
    println!(
        "nested dissection nnz(L+U) {n1:>12}  multiply-adds {f1:>14.4e}  factorise {t1:.4} s  \
         (ratios {:.3} {:.3} {:.3})",
        n1 as f64 / n0 as f64,
        f1 / f0,
        t1 / t0
    );
}
