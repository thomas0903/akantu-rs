// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the linear algebra layer: src/solver/{sparse_solver_eigen.cc,
// sparse_matrix_aij.cc, solver_vector_distributed.cc} as ported by
// akantu-linalg.

//! Each test reproduces a failing case (all fail or panic before the
//! fix). Run in debug and release (`cargo test --release`).
//!
//! * `la01_*` — malformed public inputs (`CscMatrix`, `MatrixMarket`) are
//!   errors, never panics or overflows;
//! * `la02_*` — `SolverVector::norm` agrees its local validation before the
//!   reduction (prompt common outcome on every rank);
//! * `la04_*` — faer validates its pivots (tiny nonsingular matrices solve,
//!   singular ones are refused, range failures are distinct) and no stale
//!   factorisation survives a failed refactorisation;
//! * `la05_*` — non-finite systems are refused by both backends;
//! * `la07_*` — Matrix Market dumps round-trip by default (UD-061).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::MatrixType;
use akantu_core::{Error, FatalKind};
use akantu_linalg::direct::{CscMatrix, FaerLuSolver, SparseDirectSolver};
use akantu_linalg::eigen_sparse_lu::{EigenSparseLuSolver, SparseLu};
use akantu_linalg::matrix_market::{self, MatrixMarket, MmField, MmSymmetry};
use akantu_linalg::{EquationLayout, SolverVector, SparseMatrixAij};

/// `(col_ptr, row_idx, values)`.
type RawCsc = (Vec<usize>, Vec<usize>, Vec<f64>);

fn no_panic<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => panic!("{what}: panicked instead of returning an error"),
    }
}

fn backends() -> [(&'static str, Box<dyn SparseDirectSolver>); 2] {
    [
        ("eigen", Box::new(EigenSparseLuSolver::default())),
        ("faer", Box::new(FaerLuSolver::default())),
    ]
}

fn aij(n: i64, t: &[(i64, i64, f64)]) -> SparseMatrixAij {
    let mut m = SparseMatrixAij::new("A", n, MatrixType::Unsymmetric);
    for &(i, j, v) in t {
        m.add(i, j, v).unwrap();
    }
    m
}

// ------------------------------------------------------------------

/// Malformed CSC inputs: before the fix the public fields let
/// `n = 2, col_ptr = [0]` reach `analyze_pattern` (panic) and a one-entry
/// pattern with empty values reach `factorize` (panic in `kernels.rs`).
/// The checked constructor refuses every invariant violation.
#[test]
fn la01_malformed_csc_is_refused() {
    let bad: Vec<(&str, usize, RawCsc)> = vec![
        ("short col_ptr (reviewer)", 2, (vec![0], vec![], vec![])),
        ("empty values (reviewer)", 1, (vec![0, 1], vec![0], vec![])),
        ("col_ptr[0] != 0", 1, (vec![1, 1], vec![0], vec![1.0])),
        (
            "decreasing col_ptr",
            2,
            (vec![0, 2, 1], vec![0, 1], vec![1.0, 2.0]),
        ),
        ("col_ptr[n] != nnz", 1, (vec![0, 2], vec![0], vec![1.0])),
        (
            "row out of range",
            2,
            (vec![0, 1, 2], vec![0, 2], vec![1.0, 2.0]),
        ),
        (
            "unsorted rows",
            2,
            (vec![0, 2, 2], vec![1, 0], vec![1.0, 2.0]),
        ),
        (
            "duplicate rows",
            2,
            (vec![0, 2, 2], vec![0, 0], vec![1.0, 2.0]),
        ),
        ("no col_ptr at all", 0, (vec![], vec![], vec![])),
        ("n + 1 overflows", usize::MAX, (vec![0], vec![], vec![])),
    ];
    for (what, n, (cp, ri, v)) in bad {
        let r = no_panic(what, || CscMatrix::try_new(n, cp, ri, v));
        assert!(r.is_err(), "{what}: accepted");
    }
    // valid input still accepted (and the empty matrix)
    let c = CscMatrix::try_new(2, vec![0, 1, 2], vec![0, 1], vec![1.0, 2.0]).unwrap();
    let mut lu = SparseLu::default();
    lu.analyze_pattern(&c).unwrap();
    lu.factorize(&c).unwrap();
    let mut x = [0.0; 2];
    lu.solve(&[1.0, 4.0], &mut x).unwrap();
    assert_eq!(x, [1.0, 2.0]);
    let e = CscMatrix::try_new(0, vec![0], vec![], vec![]).unwrap();
    let mut lu = SparseLu::default();
    lu.analyze_pattern(&e).unwrap();
    lu.factorize(&e).unwrap();
    // the default matrix is the valid empty one
    let mut lu = SparseLu::default();
    lu.analyze_pattern(&CscMatrix::default()).unwrap();
    lu.factorize(&CscMatrix::default()).unwrap();
}

/// `CscMatrix::from_triplets(usize::MAX, &[])` overflowed `n + 1` before
/// the fix; huge orders are now capacity errors (checked arithmetic and
/// fallible allocation), not overflows or aborts.
#[test]
fn la01_from_triplets_checks_sizes() {
    for n in [usize::MAX, usize::MAX / 2, usize::MAX / 8] {
        let r = no_panic("from_triplets", || CscMatrix::from_triplets(n, &[]));
        assert!(
            matches!(r, Err(Error::CapacityExceeded { .. })),
            "order {n}: {r:?}"
        );
    }
    assert!(CscMatrix::from_triplets(2, &[(0, 2, 1.0)]).is_err());
}

/// Public `MatrixMarket` arrays: `rows = [i64::MIN]` overflowed `i − 1` before
/// the fix, unequal arrays silently lost entries through `zip`.
#[test]
fn la01_malformed_matrix_market_arrays_are_refused() {
    let mm = |rows: Vec<i64>, cols: Vec<i64>, values: Vec<f64>| MatrixMarket {
        field: MmField::Real,
        symmetry: MmSymmetry::General,
        nrows: 2,
        ncols: 2,
        rows,
        cols,
        values,
    };
    let cases = [
        (
            "i64::MIN row (reviewer)",
            mm(vec![i64::MIN], vec![1], vec![1.0]),
        ),
        ("i64::MAX col", mm(vec![1], vec![i64::MAX], vec![1.0])),
        ("zero row", mm(vec![0], vec![1], vec![1.0])),
        ("row beyond order", mm(vec![3], vec![1], vec![1.0])),
        (
            "missing value (reviewer)",
            mm(vec![1, 2], vec![1, 2], vec![1.0]),
        ),
        ("missing column", mm(vec![1, 2], vec![1], vec![1.0, 2.0])),
        ("extra values", mm(vec![1], vec![1], vec![1.0, 2.0])),
    ];
    for (what, m) in cases {
        let r = no_panic(what, || SparseMatrixAij::from_matrix_market("M", &m));
        assert!(r.is_err(), "{what}: accepted");
    }
    let ok = SparseMatrixAij::from_matrix_market("M", &mm(vec![2], vec![1], vec![3.0])).unwrap();
    assert_eq!(ok.get(1, 0), 3.0);
}

// ------------------------------------------------------------------

const WATCHDOG: Duration = Duration::from_millis(1500);

/// Two ranks with one value each and masks `[]` /
/// `[true]`. Before the fix rank 0 returned `DimensionMismatch` while rank 1
/// waited in the all-reduce until the local world's watchdog fired. Now
/// both fail promptly, rank 0 with its own error.
#[test]
fn la02_asymmetric_norm_validation_fails_everywhere_promptly() {
    let t0 = Instant::now();
    let out = run_local(2, WATCHDOG, |comm| {
        let mut v = SolverVector::new("r", true);
        v.resize(1);
        v.set(3.0);
        let mask: &[bool] = if comm.rank() == 0 { &[] } else { &[true] };
        v.norm(mask, &comm)
    });
    assert!(t0.elapsed() < Duration::from_secs(1), "no rank waited");
    assert!(matches!(out[0], Err(Error::DimensionMismatch { .. })));
    let e1 = out[1].as_ref().unwrap_err();
    assert!(
        !matches!(e1, Error::Fatal { .. }),
        "a recoverable validation failure, not a transport failure: {e1}"
    );
    // valid input: the reduction runs (3² + 3² = 18)
    let out = run_local(2, WATCHDOG, |comm| {
        let mut v = SolverVector::new("r", true);
        v.resize(1);
        v.set(3.0);
        v.norm(&[true], &comm).unwrap()
    });
    assert_eq!(out, [18f64.sqrt(); 2]);
}

/// Inside a manager scope: the joining form agrees the validation, the
/// scope is failed on every rank afterwards (further steps fail locally,
/// promptly), and a later agreed fatal outcome is settled identically.
#[test]
fn la02_norm_in_scope_and_fatal_outcomes() {
    let t0 = Instant::now();
    let out = run_local(2, WATCHDOG, |comm| {
        let mut ph = Phases::new(&comm, "newton");
        let v = SolverVector::zeros(2);
        let mask = if comm.rank() == 1 {
            vec![true]
        } else {
            vec![true, false]
        };
        let r = v.norm_in(&mut ph, "residual-norm", &mask);
        let failed = ph.is_failed();
        let again = v.norm_in(&mut ph, "residual-norm", &[true, true]).is_err();
        (r.is_err(), failed, again)
    });
    assert!(t0.elapsed() < Duration::from_secs(1));
    assert_eq!(out, [(true, true, true); 2]);

    // an agreed fatal failure before the norm: the norm does not
    // communicate and the owner's settlement is Fatal(Agreed) everywhere
    let out = run_local(2, WATCHDOG, |comm| {
        let mut ph = Phases::new(&comm, "newton");
        let local = if comm.rank() == 0 {
            Err(Error::fatal(FatalKind::Agreed, Error::invalid("callback")))
        } else {
            Ok(())
        };
        let _ = ph.agree("callback", local, Severity::Recoverable);
        let v = SolverVector::zeros(1);
        let r = v.norm_in(&mut ph, "norm", &[true]);
        matches!(
            ph.settle(r),
            Err(Error::Fatal {
                kind: FatalKind::Agreed,
                ..
            })
        )
    });
    assert_eq!(out, [true, true]);

    // a broken communicator (transport failure): no rank communicates, the
    // standalone wrapper reports Fatal(Transport) on every rank promptly
    let t0 = Instant::now();
    let out = run_local(2, WATCHDOG, |comm| {
        let mut ph = Phases::new(&comm, "broken");
        ph.mark_broken();
        drop(ph);
        let v = SolverVector::zeros(1);
        matches!(
            v.norm(&[true], &comm),
            Err(Error::Fatal {
                kind: FatalKind::Transport,
                ..
            })
        )
    });
    assert!(t0.elapsed() < Duration::from_secs(1));
    assert_eq!(out, [true, true]);

    // a standalone call while a scope is live is refused locally (it
    // never starts an unmatched reduction); the outer scope still works
    let out = run_local(2, WATCHDOG, |comm| {
        let mut outer = Phases::new(&comm, "outer");
        let v = SolverVector::new("r", true);
        let refused = v.norm(&[], &comm).is_err() && v.is_finite_collective(&comm).is_err();
        let ok = outer.agree("after", Ok(()), Severity::Recoverable).is_ok();
        (refused, ok)
    });
    assert_eq!(out, [(true, true); 2]);
}

/// Serial use keeps working without a manager scope.
#[test]
fn la02_serial_norm_and_is_finite() {
    let mut v = SolverVector::new("r", false);
    v.resize(2);
    v.values = vec![3.0, 4.0];
    assert_eq!(v.norm(&[true, true], &Communicator::serial()).unwrap(), 5.0);
    assert!(v.norm(&[true], &Communicator::serial()).is_err());
    assert!(v.is_finite_collective(&Communicator::serial()).unwrap());
}

// ------------------------------------------------------------------

/// `A = [1e-310]` is nonsingular with condition number 1 and `A x =
/// [1e-310]` has the representable solution 1 (Eigen returns 1). Before
/// the fix faer refused it as singular because its all-ones probe solve
/// overflowed.
#[test]
fn la04_tiny_nonsingular_matrix_solves() {
    let a = aij(1, &[(0, 0, 1e-310)]);
    for (name, mut s) in backends() {
        s.analysis(&a).unwrap();
        s.factorize(&a)
            .unwrap_or_else(|e| panic!("{name}: refused [1e-310]: {e}"));
        let mut x = [0.0];
        s.solve(&[1e-310], &mut x).unwrap();
        assert_eq!(x, [1.0], "{name}");
    }
}

/// Uniform scaling `s·M` of a well-conditioned matrix: every normal scale
/// solves to the same relative accuracy; a subnormal scale with entries
/// below the pivot overflows the reciprocal pivot — an arithmetic range
/// failure (`Error::Numerical`), not a singular matrix; truly singular
/// matrices stay `Error::LinearSolver` at every scale.
#[test]
fn la04_uniform_scale_regressions() {
    let m = [
        (0, 0, 4.0),
        (0, 1, 1.0),
        (1, 0, 2.0),
        (1, 1, 5.0),
        (1, 2, -1.0),
        (2, 1, 1.0),
        (2, 2, 3.0),
    ];
    let xs = [1.0, -2.0, 0.5];
    for scale in [1e-300, 1e-150, 1.0, 1e150, 1e300] {
        let t: Vec<_> = m.iter().map(|&(i, j, v)| (i, j, v * scale)).collect();
        let a = aij(3, &t);
        let c = CscMatrix::from_aij(&a).unwrap();
        let b = c.mul_vec(&xs).unwrap();
        for (name, mut s) in backends() {
            s.analysis(&a).unwrap();
            s.factorize(&a)
                .unwrap_or_else(|e| panic!("{name} scale {scale:e}: {e}"));
            let mut x = [0.0; 3];
            s.solve(&b, &mut x).unwrap();
            for (xi, e) in x.iter().zip(xs) {
                assert!((xi - e).abs() <= 1e-14, "{name} scale {scale:e}: {x:?}");
            }
        }
    }
    // subnormal scale: 1 / (4e-310) overflows when it multiplies the entry
    // below the pivot
    let t: Vec<_> = m.iter().map(|&(i, j, v)| (i, j, v * 1e-310)).collect();
    let a = aij(3, &t);
    for (name, mut s) in backends() {
        s.analysis(&a).unwrap();
        let r = s.factorize(&a);
        assert!(
            matches!(r, Err(Error::Numerical(ref msg)) if msg.contains("arithmetic range")),
            "{name}: {r:?}"
        );
        assert!(s.solve(&[0.0; 3], &mut [0.0; 3]).is_err());
    }
    // diagonal subnormal matrix: no entry below the pivot, solvable
    let a = aij(2, &[(0, 0, 1e-310), (1, 1, 2e-310)]);
    for (name, mut s) in backends() {
        s.analysis(&a).unwrap();
        s.factorize(&a).unwrap();
        let mut x = [0.0; 2];
        s.solve(&[1e-310, 1e-310], &mut x).unwrap();
        assert_eq!(x, [1.0, 0.5], "{name}");
    }
    // exactly singular at every exactly representable scale (rank 1; powers
    // of two keep the arithmetic exact — a decimal scale such as 1e-300
    // rounds the reciprocal pivot and Eigen, bit for bit, then finds a tiny
    // nonzero second pivot, which is not a singular matrix)
    for scale in [2f64.powi(-996), 1.0, 2f64.powi(996)] {
        let a = aij(
            2,
            &[
                (0, 0, scale),
                (0, 1, 2.0 * scale),
                (1, 0, 2.0 * scale),
                (1, 1, 4.0 * scale),
            ],
        );
        for (name, mut s) in backends() {
            s.analysis(&a).unwrap();
            let r = s.factorize(&a);
            assert!(
                matches!(r, Err(Error::LinearSolver(ref m)) if m.contains("singular")),
                "{name} scale {scale:e}: {r:?}"
            );
        }
    }
}

/// Before the fix faer cleared its numeric state only after the pattern
/// check, so a failed changed-pattern refactorisation left the previous
/// factorisation usable. Every failed refactorisation (changed pattern,
/// singular values, non-finite values) now invalidates the factors.
#[test]
fn la04_failed_refactorisation_invalidates_factors() {
    let a = aij(2, &[(0, 0, 2.0), (1, 1, 4.0)]);
    let mut changed = aij(2, &[(0, 0, 2.0), (1, 1, 4.0)]);
    changed.add(0, 1, 1.0).unwrap();
    let mut singular = a.clone();
    singular.zero();
    let mut nonfinite = a.clone();
    nonfinite.add(0, 0, f64::INFINITY).unwrap();
    for bad in [&changed, &singular, &nonfinite] {
        for (name, mut s) in backends() {
            s.analysis(&a).unwrap();
            s.factorize(&a).unwrap();
            let mut x = [7.0; 2];
            s.solve(&[2.0, 4.0], &mut x).unwrap();
            assert_eq!(x, [1.0, 1.0]);
            assert!(s.factorize(bad).is_err(), "{name}");
            let mut x = [7.0; 2];
            assert!(
                s.solve(&[2.0, 4.0], &mut x).is_err(),
                "{name}: stale factorisation used after a failed refactorisation"
            );
            assert_eq!(x, [7.0; 2], "{name}: x untouched on error");
        }
    }
}

// ------------------------------------------------------------------

/// `A = [+inf]`, `b = [+inf]`: before the fix both backends reported success
/// and returned `[NaN]`. Non-finite matrices and right-hand sides are now
/// refused (`Error::Numerical`), and finite inputs whose solution
/// overflows are an arithmetic range failure; `x` is never written on
/// error.
#[test]
fn la05_non_finite_systems_are_refused() {
    for bad in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let a = aij(1, &[(0, 0, bad)]);
        for (name, mut s) in backends() {
            s.analysis(&a).unwrap();
            let r = s.factorize(&a);
            assert!(
                matches!(r, Err(Error::Numerical(_))),
                "{name}: A = [{bad}] -> {r:?}"
            );
            let mut x = [5.0];
            assert!(s.solve(&[bad], &mut x).is_err());
            assert_eq!(x, [5.0]);
        }
    }
    let a = aij(2, &[(0, 0, 1.0), (1, 1, 1.0)]);
    for bad in [f64::INFINITY, f64::NAN] {
        for (name, mut s) in backends() {
            s.analysis(&a).unwrap();
            s.factorize(&a).unwrap();
            let mut x = [5.0; 2];
            let r = s.solve(&[1.0, bad], &mut x);
            assert!(matches!(r, Err(Error::Numerical(_))), "{name}: {r:?}");
            assert_eq!(x, [5.0; 2]);
            // the factorisation stays usable for a valid right-hand side
            s.solve(&[1.0, 2.0], &mut x).unwrap();
            assert_eq!(x, [1.0, 2.0]);
        }
    }
    // finite input, overflowing solution: 1e300 / 1e-300
    let a = aij(1, &[(0, 0, 1e-300)]);
    for (name, mut s) in backends() {
        s.analysis(&a).unwrap();
        s.factorize(&a).unwrap();
        let mut x = [5.0];
        let r = s.solve(&[1e300], &mut x);
        assert!(
            matches!(r, Err(Error::Numerical(ref m)) if m.contains("arithmetic range")),
            "{name}: {r:?}"
        );
        assert_eq!(x, [5.0]);
    }
}

// ------------------------------------------------------------------

/// UD-061 (accepted, category B): Matrix Market dumps use 17 significant
/// digits by default, so `save_matrix`/`save_vector` → parse is bit-exact,
/// including `DBL_MAX` (which upstream's 15 digits turn into `inf`); the
/// upstream precision is an explicit option. Before the fix the default was
/// 15 digits.
#[test]
fn la07_matrix_market_defaults_round_trip() {
    let vals = [
        1.000_000_000_000_005,
        f64::MAX,
        -f64::MIN_POSITIVE,
        5e-324,
        0.1,
        -2.0 / 3.0,
    ];
    let n = vals.len() as i64;
    let mut a = SparseMatrixAij::new("A", n, MatrixType::Unsymmetric);
    for (k, &v) in vals.iter().enumerate() {
        a.add(k as i64, (k as i64 + 1) % n, v).unwrap();
    }
    let dir = std::env::temp_dir().join(format!("akantu-la07-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = a.save_matrix(&dir.join("a")).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains(" 0.10000000000000001\n"), "{text}");
    let back =
        SparseMatrixAij::from_matrix_market("B", &matrix_market::parse(&text).unwrap()).unwrap();
    let bits = |m: &SparseMatrixAij| m.values().iter().map(|v| v.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&back), bits(&a));
    // the explicit upstream mode is lossy
    let up = a.matrix_market_text_with(&matrix_market::MatrixMarketOptions::upstream_precision());
    assert!(up.contains(" 0.1\n") && up.contains(" 1.79769313486232e+308\n"));
    // vectors
    let layout = EquationLayout::serial(vals.len()).unwrap();
    let mut v = SolverVector::zeros(vals.len());
    v.values.copy_from_slice(&vals);
    let file = v.save_vector(&dir.join("v"), &layout, 0, 1).unwrap();
    let mm = matrix_market::parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let got: Vec<u64> = mm.values.iter().map(|x| x.to_bits()).collect();
    let want: Vec<u64> = vals.iter().map(|x| x.to_bits()).collect();
    assert_eq!(got, want);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A column pointer beyond nnz before a later smaller
/// one is an error, not a slice panic.
#[test]
fn la2_01_col_ptr_beyond_nnz_is_an_error() {
    for (n, cp, ri, v) in [
        (2usize, vec![0usize, 5, 2], vec![0usize, 1], vec![1.0, 2.0]),
        (3, vec![0, 4, 1, 2], vec![0, 1], vec![1.0, 2.0]),
        (2, vec![0, usize::MAX, 2], vec![0, 1], vec![1.0, 2.0]),
    ] {
        assert!(
            CscMatrix::try_new(n, cp.clone(), ri, v).is_err(),
            "col_ptr {cp:?}"
        );
    }
}

/// On a serial communicator the standalone `norm`
/// computes locally (no scope), like `is_finite_collective`, so it is not
/// refused while another scope is live.
#[test]
fn la2_02_serial_norm_needs_no_scope() {
    let comm = Communicator::serial();
    let mut v = SolverVector::new("r", false);
    v.resize(2);
    v.set(3.0);
    let _outer = Phases::new(&comm, "outer");
    assert_eq!(v.norm(&[true, true], &comm).unwrap(), 18f64.sqrt());
    assert_eq!(v.norm(&[true, false], &comm).unwrap(), 3.0);
    assert!(v.norm(&[true], &comm).is_err());
    assert!(v.is_finite_collective(&comm).unwrap());
}

/// `create_by_name` with an upstream type name applies
/// the D-040 kind check (a mis-registered allocator is refused).
#[test]
fn la2_04_create_by_name_checks_the_kind() {
    use akantu_linalg::direct::SparseSolverFactory;
    let mut f = SparseSolverFactory::default();
    f.register("faer", || Box::new(EigenSparseLuSolver::default()));
    assert!(
        f.create_by_name("faer").is_err(),
        "mis-registered faer must be refused"
    );
    assert!(f.create_by_name("eigen").is_ok());
}
