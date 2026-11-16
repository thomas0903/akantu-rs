// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs performance prototype (no upstream counterpart): the elemental
// matrix assembly of src/solver/sparse_matrix_aij_inline_impl.hh
// (addValues) split over threads without changing a bit.

//! `assembly_threads [n] [reps]`: a structured `n³` cube of hexahedra split
//! into six tetrahedra (3 DOFs per node, symmetric 12 × 12 elemental
//! blocks into a symmetric matrix), assembled with cached entry positions
//! (a) serially in element order and (b) by `P = 1, 2, 4, 8` threads, each
//! owning a contiguous range of matrix entries and visiting, in element
//! order, only the elements that touch its range. Every entry receives its
//! contributions in element order in both schemes, so the values are
//! bit-identical (checked). Prints the median time of `reps` runs per
//! scheme and the element range each thread visits. Prototype for the
//! design note `docs/benchmarks/perf-campaign/assembly/README.md`.

use std::time::Instant;

use akantu_core::enums::MatrixType;
use akantu_core::{GlobalIdx, Real};
use akantu_linalg::SparseMatrixAij;

fn tets(n: usize) -> Vec<[usize; 4]> {
    let id = |i: usize, j: usize, k: usize| i + (n + 1) * (j + (n + 1) * k);
    let mut t = Vec::with_capacity(6 * n * n * n);
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let v = [
                    id(i, j, k),
                    id(i + 1, j, k),
                    id(i + 1, j + 1, k),
                    id(i, j + 1, k),
                    id(i, j, k + 1),
                    id(i + 1, j, k + 1),
                    id(i + 1, j + 1, k + 1),
                    id(i, j + 1, k + 1),
                ];
                for c in [
                    [0, 1, 2, 6],
                    [0, 2, 3, 6],
                    [0, 3, 7, 6],
                    [0, 7, 4, 6],
                    [0, 4, 5, 6],
                    [0, 5, 1, 6],
                ] {
                    t.push([v[c[0]], v[c[1]], v[c[2]], v[c[3]]]);
                }
            }
        }
    }
    t
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let reps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(7);
    let elems = tets(n);
    let ndof = 3 * (n + 1).pow(3);
    let size = GlobalIdx::try_from(ndof).expect("order");
    let eq = |e: &[usize; 4]| -> [GlobalIdx; 12] {
        let mut q = [0; 12];
        for (a, &v) in e.iter().enumerate() {
            for c in 0..3 {
                q[3 * a + c] = GlobalIdx::try_from(3 * v + c).expect("eq");
            }
        }
        q
    };
    let mut m = SparseMatrixAij::new("K", size, MatrixType::Symmetric);
    for e in &elems {
        let q = eq(e);
        m.add_block_to_profile(&q, &q).expect("profile");
    }
    // Pairs of the local upper triangle (what addValues visits) and their
    // entry positions.
    let pairs: Vec<(usize, usize)> = (0..12).flat_map(|i| (i..12).map(move |j| (i, j))).collect();
    let mut pos = Vec::with_capacity(elems.len() * pairs.len());
    for e in &elems {
        let q = eq(e);
        for &(i, j) in &pairs {
            let k = m.position(q[i], q[j]).expect("in profile");
            pos.push(u32::try_from(k).expect("u32"));
        }
    }
    // Deterministic elemental values (column-major 12 × 12).
    let mut x = 0x2545_f491_4f6c_dd1du64;
    let vals: Vec<Real> = (0..elems.len() * 144)
        .map(|_| {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            (x.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 11) as Real / (1u64 << 53) as Real - 0.5
        })
        .collect();
    let nnz = m.nnz();
    let np = pairs.len();
    println!(
        "n {n}: {} tetrahedra, {ndof} DOFs, {nnz} entries, positions {} MB, values {} MB",
        elems.len(),
        pos.len() * 4 / 1_000_000,
        vals.len() * 8 / 1_000_000
    );

    // Reference: the library's add_values, element by element.
    let mut reference = m.clone();
    for (s, e) in elems.iter().enumerate() {
        let q = eq(e);
        reference
            .add_values(&q, &q, &vals[s * 144..(s + 1) * 144], MatrixType::Symmetric)
            .expect("add_values");
    }
    let ref_bits: Vec<u64> = reference.values().iter().map(|v| v.to_bits()).collect();

    // (a) serial, cached positions.
    let serial = |a: &mut [Real]| {
        for s in 0..elems.len() {
            let p = &pos[s * np..(s + 1) * np];
            let v = &vals[s * 144..(s + 1) * 144];
            for (t, &(i, j)) in pairs.iter().enumerate() {
                a[p[t] as usize] += v[i + j * 12];
            }
        }
    };
    let mut times = Vec::new();
    for _ in 0..reps {
        let mut a = vec![0.0; nnz];
        let t0 = Instant::now();
        serial(&mut a);
        times.push(t0.elapsed().as_secs_f64());
        assert_eq!(
            a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            ref_bits,
            "serial"
        );
    }
    let t_serial = median(times);
    println!("serial cached: {:.4} s", t_serial);

    // Per element: the smallest and largest entry it touches.
    let span: Vec<(u32, u32)> = (0..elems.len())
        .map(|s| {
            let p = &pos[s * np..(s + 1) * np];
            (*p.iter().min().expect("pairs"), *p.iter().max().expect("pairs"))
        })
        .collect();

    // (b) P threads, entry ranges, elements in order.
    for threads in [1usize, 2, 4, 8] {
        let bounds: Vec<usize> = (0..=threads).map(|t| t * nnz / threads).collect();
        // Element range per thread: first and last element touching it.
        let ranges: Vec<(usize, usize)> = (0..threads)
            .map(|t| {
                let (lo, hi) = (bounds[t] as u32, bounds[t + 1] as u32);
                let first = span.iter().position(|&(a, b)| b >= lo && a < hi);
                let last = span.iter().rposition(|&(a, b)| b >= lo && a < hi);
                match (first, last) {
                    (Some(f), Some(l)) => (f, l + 1),
                    _ => (0, 0),
                }
            })
            .collect();
        let mut times = Vec::new();
        for _ in 0..reps {
            let mut a = vec![0.0; nnz];
            let t0 = Instant::now();
            std::thread::scope(|sc| {
                let mut rest: &mut [Real] = &mut a;
                for t in 0..threads {
                    let (mine, tail) = rest.split_at_mut(bounds[t + 1] - bounds[t]);
                    rest = tail;
                    let (lo, hi) = (bounds[t] as u32, bounds[t + 1] as u32);
                    let (s0, s1) = ranges[t];
                    let (pos, vals, pairs, span) = (&pos, &vals, &pairs, &span);
                    sc.spawn(move || {
                        for s in s0..s1 {
                            let (a0, b0) = span[s];
                            if b0 < lo || a0 >= hi {
                                continue;
                            }
                            let p = &pos[s * np..(s + 1) * np];
                            let v = &vals[s * 144..(s + 1) * 144];
                            for (t, &(i, j)) in pairs.iter().enumerate() {
                                let k = p[t];
                                if k >= lo && k < hi {
                                    mine[(k - lo) as usize] += v[i + j * 12];
                                }
                            }
                        }
                    });
                }
            });
            times.push(t0.elapsed().as_secs_f64());
            assert_eq!(
                a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                ref_bits,
                "{threads} threads"
            );
        }
        let t = median(times);
        let visited: usize = ranges.iter().map(|r| r.1 - r.0).sum();
        println!(
            "{threads} threads: {:.4} s (x{:.2} vs serial), elements visited {:.2} x E",
            t,
            t_serial / t,
            visited as f64 / elems.len() as f64
        );
    }
}
