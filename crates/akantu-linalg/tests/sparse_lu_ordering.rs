// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test (no Akantu counterpart): the opt-in orderings of the Eigen
// SparseLU translation (D-125).

//! `SparseOrdering::NestedDissection` and `SparseOrdering::Auto` against
//! the default COLAMD ordering: solutions within the rule
//! `fast_mode/linalg/sparse_lu_ordering` of
//! `fixtures/fast_mode/linalg/tolerances.json`, residuals of both orderings
//! side by side, the same kind of error for singular and structurally
//! singular matrices, off-diagonal pivots counted for both, factors that do
//! not depend on the thread count, a permutation that depends on the
//! pattern only, and `Auto`'s choices (never in the bitwise default).

use akantu_core::Error;
use akantu_core::exec::{Exec, ExecPolicy, Grain, Precision};
use akantu_linalg::direct::CscMatrix;
use akantu_linalg::eigen_sparse_lu::{
    AUTO_MIN_ORDER, SparseLu, SparseOrdering, non_default_ordering_used,
};
use akantu_validation::fast_mode::{ModeQuantity, check_fast_mode};

const CASE: &str = "fast_mode/linalg/sparse_lu_ordering";

/// Deterministic values in `[0, 1)`.
fn rng(seed: u64) -> impl FnMut() -> f64 {
    let mut r = seed | 1;
    move || {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        (r >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A 3-D grid of `p³` nodes with `c` unknowns per node coupled with the 26
/// neighbours: off-diagonal entries in `[-1, -0.5)` (slightly unsymmetric),
/// diagonal = the absolute row sum plus `shift` (weak diagonal dominance
/// for a small shift: a stiffness-like matrix whose condition number grows
/// with `p`).
fn grid(p: usize, c: usize, shift: f64, seed: u64) -> CscMatrix {
    let mut next = rng(seed);
    let node = |i: usize, j: usize, k: usize| (i * p + j) * p + k;
    let mut t = Vec::new();
    let mut diag = vec![0.0f64; p * p * p * c];
    for i in 0..p {
        for j in 0..p {
            for k in 0..p {
                let a = node(i, j, k);
                for (di, dj, dk) in (0..27).map(|q| (q / 9, q / 3 % 3, q % 3)) {
                    let (x, y, z) = (i + di, j + dj, k + dk);
                    if x == 0 || y == 0 || z == 0 || x > p || y > p || z > p {
                        continue;
                    }
                    let b = node(x - 1, y - 1, z - 1);
                    for r in 0..c {
                        for s in 0..c {
                            let (row, col) = (a * c + r, b * c + s);
                            if row != col {
                                let v = -0.5 - 0.5 * next();
                                t.push((row, col, v));
                                diag[row] += v.abs();
                            }
                        }
                    }
                }
            }
        }
    }
    for (i, d) in diag.iter().enumerate() {
        t.push((i, i, d + shift));
    }
    CscMatrix::from_triplets(diag.len(), &t).unwrap()
}

/// A saddle-point matrix `[K Bᵀ; B 0]` (`K` = `grid(p, 1, 1.0)`, `B` a
/// constraint per fourth unknown pairing it with its successor): zero
/// diagonal entries, so both orderings need off-diagonal pivots.
fn saddle(p: usize) -> CscMatrix {
    let k = grid(p, 1, 1.0, 7);
    let n = k.n();
    let mut t = Vec::new();
    for j in 0..n {
        for q in k.col_ptr()[j]..k.col_ptr()[j + 1] {
            t.push((k.row_idx()[q], j, k.values()[q]));
        }
    }
    let mut m = n;
    for i in (0..n - 1).step_by(4) {
        for (col, v) in [(i, 1.0), (i + 1, -1.0)] {
            t.push((m, col, v));
            t.push((col, m, v));
        }
        m += 1;
    }
    CscMatrix::from_triplets(m, &t).unwrap()
}

fn rhs(n: usize) -> Vec<f64> {
    (0..n).map(|i| ((i * 13) % 17) as f64 - 8.0).collect()
}

/// Factorise and solve with `ordering` (`None`: the default) under
/// `policy`.
fn solve(
    a: &CscMatrix,
    b: &[f64],
    ordering: Option<SparseOrdering>,
    policy: ExecPolicy,
) -> (Vec<f64>, SparseLu) {
    let mut lu = SparseLu::default();
    lu.set_exec(Exec::new(policy).unwrap());
    lu.set_ordering(ordering);
    lu.analyze_pattern(a).unwrap();
    lu.factorize(a).unwrap();
    let mut x = vec![0.0; a.n()];
    lu.solve(b, &mut x).unwrap();
    (x, lu)
}

/// `‖A x − b‖₂ / ‖b‖₂`.
fn residual(a: &CscMatrix, x: &[f64], b: &[f64]) -> f64 {
    let ax = a.mul_vec(x).unwrap();
    let r: f64 = ax.iter().zip(b).map(|(p, q)| (p - q) * (p - q)).sum();
    let nb: f64 = b.iter().map(|v| v * v).sum();
    (r / nb).sqrt()
}

#[test]
fn nested_dissection_within_its_rule() {
    for (label, a) in [
        ("grid 10^3 x 3", grid(10, 3, 0.5, 1)),
        ("grid 16^3 x 1", grid(16, 1, 0.5, 2)),
        ("saddle 12^3", saddle(12)),
    ] {
        let b = rhs(a.n());
        let (xc, lc) = solve(&a, &b, None, ExecPolicy::serial());
        let (xn, ln) = solve(
            &a,
            &b,
            Some(SparseOrdering::NestedDissection),
            ExecPolicy::serial(),
        );
        assert_eq!(lc.ordering_report().used, SparseOrdering::Colamd);
        assert_eq!(ln.ordering_report().used, SparseOrdering::NestedDissection);
        let (rc, rn) = (residual(&a, &xc, &b), residual(&a, &xn, &b));
        let (fc, fn_) = (lc.factors().unwrap(), ln.factors().unwrap());
        eprintln!(
            "{label}: n {}, residual COLAMD {rc:.2e} ND {rn:.2e}, nnz(L+U) {} / {}, \
             off-diagonal pivots {} / {}",
            a.n(),
            fc.nnz_l + fc.nnz_u,
            fn_.nnz_l + fn_.nnz_u,
            lc.off_diagonal_pivots(),
            ln.off_diagonal_pivots()
        );
        assert!(rc < 1e-14 && rn < 1e-14, "{label}: residuals {rc:e} {rn:e}");
        let reports = check_fast_mode(
            CASE,
            &[ModeQuantity::nodal("x", 1, xc)],
            &[ModeQuantity::nodal("x", 1, xn)],
            true,
        );
        for r in reports {
            eprintln!(
                "{label}: {} exact of {}, worst ratio to the bound {:.3e}",
                r.n_exact,
                r.n_compared,
                r.worst.as_ref().map_or(0.0, |w| w.ratio)
            );
        }
    }
}

#[test]
fn saddle_point_needs_off_diagonal_pivots_with_both_orderings() {
    let a = saddle(8);
    let b = rhs(a.n());
    let (_, lc) = solve(&a, &b, None, ExecPolicy::serial());
    let (_, ln) = solve(
        &a,
        &b,
        Some(SparseOrdering::NestedDissection),
        ExecPolicy::serial(),
    );
    assert!(lc.off_diagonal_pivots() > 0);
    assert!(ln.off_diagonal_pivots() > 0);
    // a diagonally dominant matrix pivots on the diagonal with both
    let g = grid(8, 3, 1.0, 3);
    let bg = rhs(g.n());
    let (_, gc) = solve(&g, &bg, None, ExecPolicy::serial());
    let (_, gn) = solve(
        &g,
        &bg,
        Some(SparseOrdering::NestedDissection),
        ExecPolicy::serial(),
    );
    assert_eq!((gc.off_diagonal_pivots(), gn.off_diagonal_pivots()), (0, 0));
}

/// The error of a failed factorisation with `ordering`.
fn factorize_error(a: &CscMatrix, ordering: SparseOrdering) -> Error {
    let mut lu = SparseLu::default();
    lu.set_ordering(Some(ordering));
    lu.analyze_pattern(a).unwrap();
    let e = lu.factorize(a).unwrap_err();
    assert!(lu.factors().is_none());
    e
}

#[test]
fn singular_matrices_give_the_same_kind_of_error() {
    let base = grid(6, 2, 0.5, 4);
    let n = base.n();
    let entries = |a: &CscMatrix| -> Vec<(usize, usize, f64)> {
        let mut t = Vec::new();
        for j in 0..a.n() {
            for q in a.col_ptr()[j]..a.col_ptr()[j + 1] {
                t.push((a.row_idx()[q], j, a.values()[q]));
            }
        }
        t
    };
    // structurally singular: row 5 has no entry
    let t: Vec<_> = entries(&base).into_iter().filter(|e| e.0 != 5).collect();
    let structural = CscMatrix::from_triplets(n, &t).unwrap();
    // singular by value: column 9 stored with zeros
    let t: Vec<_> = entries(&base)
        .into_iter()
        .map(|(i, j, v)| (i, j, if j == 9 { 0.0 } else { v }))
        .collect();
    let zero_column = CscMatrix::from_triplets(n, &t).unwrap();
    // numerically singular: an appended rank-one block [[1, 2], [2, 4]]
    // (exact cancellation whichever of its columns comes first)
    let mut t = entries(&base);
    t.extend([
        (n, n, 1.0),
        (n, n + 1, 2.0),
        (n + 1, n, 2.0),
        (n + 1, n + 1, 4.0),
    ]);
    let rank_one = CscMatrix::from_triplets(n + 2, &t).unwrap();
    for (label, a) in [
        ("structurally singular", &structural),
        ("zero column", &zero_column),
        ("rank-one block", &rank_one),
    ] {
        let ec = factorize_error(a, SparseOrdering::Colamd);
        let en = factorize_error(a, SparseOrdering::NestedDissection);
        assert!(
            matches!((&ec, &en), (Error::LinearSolver(c), Error::LinearSolver(d))
                if c.contains("singular") && d.contains("singular")),
            "{label}: {ec} / {en}"
        );
    }
}

/// The factor arrays and the solution as bits.
fn bits(lu: &SparseLu, x: &[f64]) -> Vec<u64> {
    let f = lu.factors().unwrap();
    f.lusup
        .iter()
        .chain(&f.ucol)
        .chain(x)
        .map(|v| v.to_bits())
        .chain(
            f.perm_r
                .iter()
                .chain(&f.lsub)
                .chain(&f.usub)
                .chain(lu.perm_c())
                .map(|&i| i as u64),
        )
        .collect()
}

#[test]
fn nested_dissection_factors_are_thread_invariant() {
    for a in [grid(9, 3, 0.5, 5), grid(14, 1, 0.5, 6)] {
        let b = rhs(a.n());
        let (x, lu) = solve(
            &a,
            &b,
            Some(SparseOrdering::NestedDissection),
            ExecPolicy::serial(),
        );
        let want = bits(&lu, &x);
        for threads in [1, 2, 3, 4, 8] {
            for grain in [Grain::Auto, Grain::Finest] {
                for min in [None, Some(0)] {
                    let mut lu = SparseLu::default();
                    let p = ExecPolicy::serial()
                        .with_threads(threads)
                        .unwrap()
                        .with_grain(grain);
                    lu.set_exec(Exec::new(p).unwrap());
                    lu.set_threaded_update_min(min);
                    lu.set_ordering(Some(SparseOrdering::NestedDissection));
                    lu.analyze_pattern(&a).unwrap();
                    lu.factorize(&a).unwrap();
                    let mut x = vec![0.0; a.n()];
                    lu.solve(&b, &mut x).unwrap();
                    assert!(
                        bits(&lu, &x) == want,
                        "n {}: {threads} threads, {grain:?}, threshold {min:?}",
                        a.n()
                    );
                }
            }
        }
    }
}

#[test]
fn the_permutation_depends_on_the_pattern_only() {
    let a = grid(10, 3, 0.5, 8);
    // same pattern, other values
    let b = grid(10, 3, 2.0, 9);
    let perm = |m: &CscMatrix| {
        let mut lu = SparseLu::default();
        lu.set_ordering(Some(SparseOrdering::NestedDissection));
        lu.analyze_pattern(m).unwrap();
        lu.perm_c().to_vec()
    };
    let p = perm(&a);
    // the process records that a non-default ordering ran
    assert!(non_default_ordering_used());
    assert_eq!(p, perm(&a));
    assert_eq!(p, perm(&b));
    let mut sorted = p.clone();
    sorted.sort_unstable();
    assert!(sorted.iter().enumerate().all(|(i, &v)| v == i as i64));
}

#[test]
fn auto_chooses_by_predicted_work_and_never_in_the_bitwise_default() {
    let big = grid(14, 1, 0.5, 10);
    let small = grid(8, 1, 0.5, 11);
    assert!(big.n() >= AUTO_MIN_ORDER && small.n() < AUTO_MIN_ORDER);
    let analyse = |a: &CscMatrix, o: Option<SparseOrdering>, p: ExecPolicy| {
        let mut lu = SparseLu::default();
        lu.set_exec(Exec::new(p).unwrap());
        lu.set_ordering(o);
        lu.analyze_pattern(a).unwrap();
        lu
    };
    let bitwise = ExecPolicy::serial();
    let fast = bitwise.with_precision(Precision::Fast);
    // unset ordering: COLAMD in the bitwise mode, whatever the size
    let lu = analyse(&big, None, bitwise);
    assert_eq!(lu.effective_ordering(), SparseOrdering::Colamd);
    assert_eq!(lu.ordering_report().used, SparseOrdering::Colamd);
    assert!(lu.ordering_report().predicted.is_none());
    // the same as an explicit COLAMD, bit for bit
    let explicit = analyse(&big, Some(SparseOrdering::Colamd), bitwise);
    assert_eq!(lu.perm_c(), explicit.perm_c());
    // unset ordering in the fast mode: Auto
    let lu = analyse(&big, None, fast);
    assert_eq!(lu.effective_ordering(), SparseOrdering::Auto);
    let r = lu.ordering_report();
    let [c, d] = r.predicted.expect("predictions above the size threshold");
    eprintln!(
        "auto on a 15^3 grid: predicted work COLAMD {:.3e}, ND {:.3e}",
        c.1, d.1
    );
    assert_eq!(r.used, SparseOrdering::NestedDissection);
    // below the size threshold Auto keeps COLAMD without predicting
    let lu = analyse(&small, Some(SparseOrdering::Auto), bitwise);
    assert_eq!(lu.ordering_report().used, SparseOrdering::Colamd);
    assert!(lu.ordering_report().predicted.is_none());
    assert_eq!(
        lu.perm_c(),
        analyse(&small, None, bitwise).perm_c(),
        "below the threshold Auto is COLAMD"
    );
    // a matrix that needs off-diagonal pivots keeps COLAMD: zero diagonal
    // entries (saddle point), or one column whose diagonal is not its
    // largest entry
    let sp = saddle(14);
    assert!(sp.n() >= AUTO_MIN_ORDER);
    let lu = analyse(&sp, Some(SparseOrdering::Auto), bitwise);
    assert_eq!(lu.ordering_report().used, SparseOrdering::Colamd);
    assert!(lu.ordering_report().predicted.is_none());
    let (n, cp, ri, mut v) = big.clone().into_parts();
    for q in cp[7]..cp[8] {
        if ri[q] == 7 {
            v[q] = 1e-3;
        }
    }
    let weak = CscMatrix::try_new(n, cp, ri, v).unwrap();
    let lu = analyse(&weak, Some(SparseOrdering::Auto), bitwise);
    assert_eq!(lu.ordering_report().used, SparseOrdering::Colamd);
}

#[test]
fn ordering_names_parse() {
    for o in [
        SparseOrdering::Colamd,
        SparseOrdering::NestedDissection,
        SparseOrdering::Auto,
    ] {
        assert_eq!(o.name().parse::<SparseOrdering>().unwrap(), o);
    }
    assert_eq!(
        "nd".parse::<SparseOrdering>().unwrap(),
        SparseOrdering::NestedDissection
    );
    assert!("metis".parse::<SparseOrdering>().is_err());
}
