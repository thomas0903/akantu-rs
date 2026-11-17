// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test (no Akantu counterpart): the fast mode of the Eigen
// SparseLU translation against its bitwise default (D-122, D-123).

//! `Precision::Fast` for the sparse direct solver: the FAST MODE kernels
//! (fused multiply-adds in the panel products, the triangular panel solves
//! and the GEMV) against the bitwise factorisation and solve, under the
//! rule `fast_mode/linalg/sparse_lu_stencil` of
//! `fixtures/fast_mode/linalg/tolerances.json`; the fast results do not
//! depend on the thread count.

use akantu_core::exec::{Exec, ExecPolicy, Grain, Precision, SimdLevel};
use akantu_linalg::direct::CscMatrix;
use akantu_linalg::eigen_sparse_lu::{SparseLu, SparseOrdering};
use akantu_validation::fast_mode::{ModeQuantity, check_fast_mode};

const CASE: &str = "fast_mode/linalg/sparse_lu_stencil";

/// A 3-D 7-point stencil of `k³` unknowns, unsymmetric, diagonally
/// dominant (Gershgorin: the eigenvalues lie in `[0.5, 13.4]`).
fn stencil3(k: usize) -> CscMatrix {
    let id = |i: usize, j: usize, l: usize| (i * k + j) * k + l;
    let mut t = Vec::new();
    for i in 0..k {
        for j in 0..k {
            for l in 0..k {
                let p = id(i, j, l);
                t.push((p, p, 6.5 + ((p * 7) % 5) as f64 * 0.1));
                let nb = [
                    (i.wrapping_sub(1), j, l),
                    (i + 1, j, l),
                    (i, j.wrapping_sub(1), l),
                    (i, j + 1, l),
                    (i, j, l.wrapping_sub(1)),
                    (i, j, l + 1),
                ];
                for (q, &(a, b, c)) in nb.iter().enumerate() {
                    if a < k && b < k && c < k {
                        t.push((p, id(a, b, c), -1.0 + 0.05 * q as f64));
                    }
                }
            }
        }
    }
    CscMatrix::from_triplets(k * k * k, &t).unwrap()
}

fn run(a: &CscMatrix, b: &[f64], policy: ExecPolicy) -> Vec<ModeQuantity> {
    let mut lu = SparseLu::default();
    lu.set_exec(Exec::new(policy).unwrap());
    // the FMA kernels alone: COLAMD in both modes (an unset ordering is
    // `auto` in the fast mode, D-125; its rule is sparse_lu_ordering)
    lu.set_ordering(Some(SparseOrdering::Colamd));
    lu.analyze_pattern(a).unwrap();
    lu.factorize(a).unwrap();
    let mut x = vec![0.0; a.n()];
    lu.solve(b, &mut x).unwrap();
    vec![ModeQuantity::nodal("x", 1, x)]
}

#[test]
fn fast_mode_sparse_lu_within_its_rule() {
    let a = stencil3(16);
    let b: Vec<f64> = (0..a.n()).map(|i| ((i * 13) % 17) as f64 - 8.0).collect();
    let bitwise = run(&a, &b, ExecPolicy::serial());
    let fast_policy = ExecPolicy::serial().with_precision(Precision::Fast);
    let fast = run(&a, &b, fast_policy);
    // the FMA kernels exist for the AVX2 and AVX-512 levels
    let fma = matches!(
        fast_policy.simd_token().level(),
        SimdLevel::Avx2 | SimdLevel::Avx512
    );
    for r in check_fast_mode(CASE, &bitwise, &fast, fma) {
        let w = r.worst.as_ref().map_or(0.0, |w| w.ratio);
        eprintln!(
            "{}: {} entries, {} exact, worst ratio {w:.3e}, max rel {:?}",
            r.quantity,
            r.n_compared,
            r.n_exact,
            r.max_rel_err.as_ref().map(|d| d.rel_err)
        );
    }
    // the fast kernels' results do not depend on the thread count
    for threads in [2, 4] {
        let p = fast_policy
            .with_threads(threads)
            .unwrap()
            .with_grain(Grain::Finest);
        assert_eq!(run(&a, &b, p), fast, "threads={threads}");
    }
}

/// Factor arrays and solution of `lu` as bits.
fn bits(lu: &mut SparseLu, a: &CscMatrix, b: &[f64]) -> Vec<u64> {
    lu.factorize(a).unwrap();
    let mut x = vec![0.0; a.n()];
    lu.solve(b, &mut x).unwrap();
    let f = lu.factors().unwrap();
    f.lusup
        .iter()
        .chain(&f.ucol)
        .chain(&x)
        .map(|v| v.to_bits())
        .chain(
            f.perm_r
                .iter()
                .chain(&f.lsub)
                .chain(&f.usub)
                .map(|&i| i as u64),
        )
        .collect()
}

/// A fast factorisation followed by a bitwise one on the same solver gives
/// the fresh bitwise factors and solution (no state of the fast mode
/// survives), whether the mode comes from the executor (`set_exec`, 3
/// threads, every update threaded) or from an explicit token
/// (`set_fast_mode`); the fast runs differ from the bitwise one where the
/// CPU has an FMA instance, so the test cannot pass without the fast
/// kernels running.
#[test]
fn bitwise_after_fast_on_the_same_solver() {
    let a = stencil3(12);
    let b: Vec<f64> = (0..a.n()).map(|i| ((i * 11) % 13) as f64 - 6.0).collect();
    let mut fresh = SparseLu::default();
    fresh.analyze_pattern(&a).unwrap();
    let want = bits(&mut fresh, &a, &b);
    let fma = matches!(
        ExecPolicy::serial().simd_token().level(),
        SimdLevel::Avx2 | SimdLevel::Avx512
    );
    let threads = ExecPolicy::serial().with_threads(3).unwrap();
    // through the executor's precision
    let mut lu = SparseLu::default();
    lu.set_threaded_update_min(Some(0));
    lu.analyze_pattern(&a).unwrap();
    lu.set_exec(Exec::new(threads.with_precision(Precision::Fast)).unwrap());
    let fast = bits(&mut lu, &a, &b);
    assert_eq!(
        fast != want,
        fma,
        "the fast mode changes bits iff an FMA instance exists"
    );
    lu.set_exec(Exec::new(threads).unwrap());
    assert!(
        bits(&mut lu, &a, &b) == want,
        "Fast -> Bitwise through set_exec"
    );
    // through an explicit token
    let token = Exec::new(ExecPolicy::serial().with_precision(Precision::Fast))
        .unwrap()
        .fast_token();
    let mut lu = SparseLu::default();
    lu.analyze_pattern(&a).unwrap();
    lu.set_fast_mode(token);
    let fast = bits(&mut lu, &a, &b);
    assert_eq!(fast != want, fma);
    lu.set_fast_mode(None);
    assert!(
        bits(&mut lu, &a, &b) == want,
        "Fast -> Bitwise through set_fast_mode"
    );
}
