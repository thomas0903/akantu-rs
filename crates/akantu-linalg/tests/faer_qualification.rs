// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Qualification of the
// pure-Rust faer backend (decision record D-014, ARCHITECTURE) against
// the oracle's SparseSolverEigen solutions in fixtures/solver/*, and of both
// direct backends on manufactured, singular and indefinite systems. The
// acceptance rules are stated in docs/NUMERICAL_VALIDATION.md, "Sparse
// algebra and direct solvers".

//! faer qualification protocol (SOLV-004 substitute): backward error
//! (LAPACK `dget02` ratio), conditioning-aware forward error against the
//! oracle solution, componentwise residual (reported), singular matrices
//! refused, indefinite systems solved.

mod solver_oracle;

use akantu_core::enums::MatrixType;
use akantu_linalg::direct::{CscMatrix, FaerLuSolver, SparseDirectSolver};
use akantu_linalg::eigen_sparse_lu::EigenSparseLuSolver;
use akantu_linalg::{EquationLayout, SparseMatrixAij};
use akantu_validation::reduce::NanMinMax;
use solver_oracle::*;

const EPS: f64 = f64::EPSILON;
/// LAPACK testing threshold for the residual ratio (`THRESH = 30` in the
/// LAPACK test-suite input files, LAWN 41).
const DGET02_THRESH: f64 = 30.0;

fn norm1_mat(a: &CscMatrix) -> f64 {
    (0..a.n())
        .map(|j| {
            (a.col_ptr()[j]..a.col_ptr()[j + 1])
                .map(|p| a.values()[p].abs())
                .sum::<f64>()
        })
        .fold(0.0, NanMinMax::nan_max)
}

fn norm1(v: &[f64]) -> f64 {
    v.iter().map(|x| x.abs()).sum()
}

fn norm_inf(v: &[f64]) -> f64 {
    v.iter().fold(0.0, |m, x| m.nan_max(x.abs()))
}

/// Residual `b - A x` accumulated in a compensated (two-sum) way so that
/// the check itself does not dominate the measured error.
fn residual(a: &CscMatrix, x: &[f64], b: &[f64]) -> Vec<f64> {
    let mut s = b.to_vec();
    let mut c = vec![0.0; a.n()];
    for (j, &xj) in x.iter().enumerate() {
        for p in a.col_ptr()[j]..a.col_ptr()[j + 1] {
            let i = a.row_idx()[p];
            let prod = a.values()[p] * xj;
            // exact error of the product (TwoProduct): test-side measurement
            // only, never part of a computation compared with the oracle
            let err = a.values()[p].mul_add(xj, -prod);
            for t in [-prod, -err] {
                let y = s[i] + t;
                let bb = y - s[i];
                c[i] += (s[i] - (y - bb)) + (t - bb);
                s[i] = y;
            }
        }
    }
    s.iter().zip(&c).map(|(a, b)| a + b).collect()
}

/// LAPACK `dget02` ratio `‖b − A x‖₁ / (‖A‖₁ ‖x‖₁ ε)` — exactly LAPACK's
/// definition (**no factor `n`**), evaluated as LAPACK
/// does by successive divisions `((‖r‖₁ / ‖A‖₁) / ‖x‖₁) / ε` (no overflow
/// or underflow of an intermediate product). Zero cases: a zero residual
/// gives 0 (also for `A = 0` or `x = 0`); a nonzero residual with `‖A‖₁ =
/// 0` or `‖x‖₁ = 0` gives LAPACK's `1/ε` (a failure against 30).
fn dget02(a: &CscMatrix, x: &[f64], b: &[f64]) -> f64 {
    let rnorm = norm1(&residual(a, x, b));
    if rnorm == 0.0 {
        return 0.0;
    }
    let (anorm, xnorm) = (norm1_mat(a), norm1(x));
    if anorm <= 0.0 || xnorm <= 0.0 {
        return 1.0 / EPS;
    }
    ((rnorm / anorm) / xnorm) / EPS
}

/// Normwise backward error `‖r‖∞ / (‖A‖∞ ‖x‖∞ + ‖b‖∞)`.
fn eta(a: &CscMatrix, x: &[f64], b: &[f64]) -> f64 {
    let r = residual(a, x, b);
    let mut row = vec![0.0; a.n()];
    for j in 0..a.n() {
        for p in a.col_ptr()[j]..a.col_ptr()[j + 1] {
            row[a.row_idx()[p]] += a.values()[p].abs();
        }
    }
    norm_inf(&r) / (norm_inf(&row) * norm_inf(x) + norm_inf(b))
}

/// Componentwise (Oettli–Prager) backward error `max |r_i| / (|A||x| + |b|)_i`.
fn omega(a: &CscMatrix, x: &[f64], b: &[f64]) -> f64 {
    let r = residual(a, x, b);
    let mut d: Vec<f64> = b.iter().map(|v| v.abs()).collect();
    for (j, &xj) in x.iter().enumerate() {
        for p in a.col_ptr()[j]..a.col_ptr()[j + 1] {
            d[a.row_idx()[p]] += (a.values()[p] * xj).abs();
        }
    }
    r.iter()
        .zip(&d)
        .map(|(ri, di)| {
            if *di == 0.0 {
                if *ri == 0.0 { 0.0 } else { f64::INFINITY }
            } else {
                ri.abs() / di
            }
        })
        .fold(0.0, NanMinMax::nan_max)
}

/// Estimate of `κ∞(A) = ‖A‖∞ ‖A⁻¹‖∞` with `A⁻¹` from `n` solves of
/// `solver` (`‖A⁻¹‖∞` = max row sum of `|A⁻¹|`, accumulated from the
/// computed columns). The computed inverse carries its own rounding: this
/// is an **estimate** of the condition number, not a certified bound.
fn kappa_inf(a: &CscMatrix, solver: &mut dyn SparseDirectSolver) -> f64 {
    let n = a.n();
    let mut rows = vec![0.0; n];
    let mut e = vec![0.0; n];
    let mut col = vec![0.0; n];
    for j in 0..n {
        e[j] = 1.0;
        solver.solve(&e, &mut col).unwrap();
        e[j] = 0.0;
        for (r, c) in rows.iter_mut().zip(&col) {
            *r += c.abs();
        }
    }
    let mut arow = vec![0.0; n];
    for j in 0..n {
        for p in a.col_ptr()[j]..a.col_ptr()[j + 1] {
            arow[a.row_idx()[p]] += a.values()[p].abs();
        }
    }
    norm_inf(&arow) * norm_inf(&rows)
}

fn aij_from_triplets(n: usize, t: &[(usize, usize, f64)]) -> SparseMatrixAij {
    let mut m = SparseMatrixAij::new("A", n as i64, MatrixType::Unsymmetric);
    for &(i, j, v) in t {
        m.add(i as i64, j as i64, v).unwrap();
    }
    m
}

struct Report {
    case: String,
    n: usize,
    kappa: f64,
    ratio: f64,
    eta_faer: f64,
    eta_eigen: f64,
    omega_faer: f64,
    fwd: f64,
    bound: f64,
}

/// Rounding allowance of the forward comparison: `4u = 2ε` (`u = ε/2`),
/// relative to `‖x_oracle‖∞`. Two correctly rounded representations of the
/// same exact solution may differ componentwise by up to `2u` relative, and
/// the measurement itself (the subtraction, the division by `‖x_o‖∞`, the
/// evaluation of the bound) adds a few more units of `u`; one explicit
/// allowance covers both (NUMERICAL_VALIDATION).
const FWD_ROUNDING_ALLOWANCE: f64 = 2.0 * EPS;

/// Finite-perturbation forward bound (NUMERICAL_VALIDATION rule 2): with `ηf`, `ηo` the normwise backward errors of the faer
/// and oracle solutions and `κ = κ∞(A)`,
/// `‖xf − xo‖∞/‖xo‖∞ ≤ 2κ(ηf + ηo) / ((1 − κηf)(1 − ηo))`, valid for
/// `κηf < 1`, `ηo < 1` and `xo ≠ 0` (it follows from `A(xf − xo) =
/// rf − ro`, `‖rᵢ‖ ≤ ηᵢ(‖A‖‖xᵢ‖ + ‖b‖)` and `‖b‖/‖A‖ ≤ ((1 + ηo)/(1 −
/// ηo))‖xo‖`). Returns `None` outside its validity domain.
fn forward_bound(kappa: f64, eta_f: f64, eta_o: f64) -> Option<f64> {
    (kappa * eta_f < 1.0 && eta_o < 1.0)
        .then(|| 2.0 * kappa * (eta_f + eta_o) / ((1.0 - kappa * eta_f) * (1.0 - eta_o)))
}

/// The protocol on one system: faer must (1) pass the dget02 ratio,
/// (2) agree with the oracle (Eigen) solution within the finite
/// perturbation bound [`forward_bound`] plus [`FWD_ROUNDING_ALLOWANCE`].
fn qualify(case: &str, m: &SparseMatrixAij, b: &[f64], x_oracle: &[f64]) -> Report {
    let a = CscMatrix::from_aij(m).unwrap();
    let mut s = FaerLuSolver::default();
    s.analysis(m).unwrap();
    s.factorize(m).unwrap();
    let mut x = vec![0.0; a.n()];
    s.solve(b, &mut x).unwrap();
    let ratio = dget02(&a, &x, b);
    let eta_faer = eta(&a, &x, b);
    let eta_eigen = eta(&a, x_oracle, b);
    let kappa = kappa_inf(&a, &mut s);
    let diff: Vec<f64> = x.iter().zip(x_oracle).map(|(p, q)| p - q).collect();
    assert!(norm_inf(x_oracle) > 0.0, "{case}: zero reference solution");
    let fwd = norm_inf(&diff) / norm_inf(x_oracle);
    let bound = forward_bound(kappa, eta_faer, eta_eigen)
        .unwrap_or_else(|| panic!("{case}: outside the bound's validity domain"))
        + FWD_ROUNDING_ALLOWANCE;
    let r = Report {
        case: case.to_owned(),
        n: a.n(),
        kappa,
        ratio,
        eta_faer,
        eta_eigen,
        omega_faer: omega(&a, &x, b),
        fwd,
        bound,
    };
    assert!(ratio < DGET02_THRESH, "{case}: dget02 ratio {ratio}");
    assert!(
        fwd <= bound,
        "{case}: forward difference {fwd:e} > bound {bound:e}"
    );
    r
}

#[test]
fn faer_against_oracle_solutions() {
    let mut reports = Vec::new();
    for f in cases("aij") {
        let k = replay_k_bc(&f);
        reports.push(qualify(
            f.case_id(),
            &k,
            &f64s(&f, "eigen/b"),
            &f64s(&f, "eigen/x"),
        ));
    }
    for f in cases("lu") {
        if !param(&f, "lu_info_success").as_bool().unwrap() {
            // singular: faer must refuse it too
            let n = param(&f, "n").as_i64().unwrap() as usize;
            let (r, c, v) = (i64s(&f, "A/rows"), i64s(&f, "A/cols"), f64s(&f, "A/values"));
            let t: Vec<_> = r
                .iter()
                .zip(&c)
                .zip(&v)
                .map(|((&i, &j), &x)| (i as usize, j as usize, x))
                .collect();
            let m = aij_from_triplets(n, &t);
            let mut s = FaerLuSolver::default();
            let res = s.analysis(&m).and_then(|()| s.factorize(&m));
            assert!(
                res.is_err(),
                "{}: faer accepted a singular matrix",
                f.case_id()
            );
            eprintln!(
                "{}: singular, refused by faer ({})",
                f.case_id(),
                res.unwrap_err()
            );
            continue;
        }
        let n = param(&f, "n").as_i64().unwrap() as usize;
        let (r, c, v) = (i64s(&f, "A/rows"), i64s(&f, "A/cols"), f64s(&f, "A/values"));
        let t: Vec<_> = r
            .iter()
            .zip(&c)
            .zip(&v)
            .map(|((&i, &j), &x)| (i as usize, j as usize, x))
            .collect();
        // duplicates summed in triplet order, as setFromTriplets (and AIJ add)
        let m = aij_from_triplets(n, &t);
        reports.push(qualify(f.case_id(), &m, &f64s(&f, "b"), &f64s(&f, "lu/x")));
    }
    eprintln!(
        "case | n | kappa_inf | dget02 ratio | eta faer | eta eigen | omega faer | fwd diff | bound"
    );
    for r in &reports {
        eprintln!(
            "{} | {} | {:.3e} | {:.3} | {:.2e} | {:.2e} | {:.2e} | {:.2e} | {:.2e}",
            r.case, r.n, r.kappa, r.ratio, r.eta_faer, r.eta_eigen, r.omega_faer, r.fwd, r.bound
        );
    }
    assert_eq!(reports.len(), 12);
}

/// 5-point Laplacian on an `nx × ny` grid with Dirichlet rows on the
/// boundary (unit diagonal, as `applyBoundary`), symmetric storage.
fn laplacian(nx: usize, ny: usize) -> SparseMatrixAij {
    let n = nx * ny;
    let mut k = SparseMatrixAij::new("K", n as i64, MatrixType::Symmetric);
    let id = |i: usize, j: usize| (j * nx + i) as i64;
    for j in 0..ny {
        for i in 0..nx {
            let r = id(i, j);
            k.add(r, r, 4.0).unwrap();
            if i + 1 < nx {
                k.add(r, id(i + 1, j), -1.0).unwrap();
            }
            if j + 1 < ny {
                k.add(r, id(i, j + 1), -1.0).unwrap();
            }
        }
    }
    let layout = EquationLayout::serial(n).unwrap();
    let blocked: Vec<bool> = (0..n)
        .map(|r| {
            let (i, j) = (r % nx, r / nx);
            i == 0 || j == 0 || i + 1 == nx || j + 1 == ny
        })
        .collect();
    k.apply_boundary(&layout, &blocked, 1.0).unwrap();
    k
}

/// Manufactured solutions on SPD Laplacians: both backends recover `x*`
/// within `κ∞ η` (first order) and pass the residual ratio; the Eigen
/// translation and faer agree within the same bound.
#[test]
fn manufactured_spd_laplacian_solutions() {
    for (nx, ny) in [(12, 9), (40, 31)] {
        let k = laplacian(nx, ny);
        let a = CscMatrix::from_aij(&k).unwrap();
        let n = a.n();
        let xs: Vec<f64> = (0..n)
            .map(|r| ((r * 37 % 101) as f64 / 50.0 - 1.0) * (1.0 + (r % 7) as f64))
            .collect();
        let b = a.mul_vec(&xs).unwrap();
        for backend in ["eigen", "faer"] {
            let mut s: Box<dyn SparseDirectSolver> = if backend == "eigen" {
                Box::new(EigenSparseLuSolver::default())
            } else {
                Box::new(FaerLuSolver::default())
            };
            s.analysis(&k).unwrap();
            s.factorize(&k).unwrap();
            let mut x = vec![0.0; n];
            s.solve(&b, &mut x).unwrap();
            let ratio = dget02(&a, &x, &b);
            let e = eta(&a, &x, &b);
            let kap = kappa_inf(&a, s.as_mut());
            // x* is an exact solution of the perturbed system A x* = b + Δb,
            // b = fl(A x*): ‖Δb‖∞ ≤ γ_5 ‖|A||x*|‖∞ ≤ γ_5 ‖A‖∞‖x*‖∞ (at most 5
            // entries per row), so x* has backward error η* ≤ γ_5 and the
            // finite bound applies with (ηf, ηo) = (η, η*).
            let gamma5 = 5.0 * EPS / 2.0 / (1.0 - 5.0 * EPS / 2.0);
            let diff: Vec<f64> = x.iter().zip(&xs).map(|(p, q)| p - q).collect();
            let fwd = norm_inf(&diff) / norm_inf(&xs);
            let bound = forward_bound(kap, e, gamma5).unwrap() + FWD_ROUNDING_ALLOWANCE;
            assert!(ratio < DGET02_THRESH, "{backend} {nx}x{ny}: ratio {ratio}");
            assert!(
                fwd <= bound,
                "{backend} {nx}x{ny}: fwd {fwd:e} > {bound:e} (kappa {kap:e}, eta {e:e})"
            );
            eprintln!(
                "{backend} laplacian {nx}x{ny}: n {n} kappa {kap:.2e} ratio {ratio:.3} eta {e:.2e} fwd {fwd:.2e}"
            );
        }
    }
}

/// Symmetric indefinite (saddle-point) system `[[K, Bᵀ], [B, 0]]`: the zero
/// diagonal block needs row pivoting; both backends solve it.
#[test]
fn indefinite_saddle_point_system() {
    let nk = 30;
    let nb = 6;
    let n = nk + nb;
    let mut m = SparseMatrixAij::new("S", n as i64, MatrixType::Symmetric);
    for i in 0..nk {
        m.add(i as i64, i as i64, 2.0 + (i % 3) as f64).unwrap();
        if i + 1 < nk {
            m.add(i as i64, i as i64 + 1, -1.0).unwrap();
        }
    }
    for c in 0..nb {
        for t in 0..3 {
            m.add((nk + c) as i64, (c * 5 + t) as i64, 1.0 + t as f64)
                .unwrap();
        }
    }
    let a = CscMatrix::from_aij(&m).unwrap();
    let xs: Vec<f64> = (0..n).map(|i| 1.0 + i as f64 / 7.0).collect();
    let b = a.mul_vec(&xs).unwrap();
    for backend in ["eigen", "faer"] {
        let mut s: Box<dyn SparseDirectSolver> = if backend == "eigen" {
            Box::new(EigenSparseLuSolver::default())
        } else {
            Box::new(FaerLuSolver::default())
        };
        s.analysis(&m).unwrap();
        s.factorize(&m).unwrap();
        let mut x = vec![0.0; n];
        s.solve(&b, &mut x).unwrap();
        let ratio = dget02(&a, &x, &b);
        assert!(ratio < DGET02_THRESH, "{backend}: ratio {ratio}");
        eprintln!("{backend} saddle point: n {n} dget02 {ratio:.3}");
        let fwd = x
            .iter()
            .zip(&xs)
            .fold(0.0f64, |w, (p, q)| w.nan_max((p - q).abs()))
            / norm_inf(&xs);
        assert!(fwd < 1e-12, "{backend}: fwd {fwd:e}");
    }
}

/// A promoted (UD-034) matrix forces a new analysis with the general
/// factorisation; a value change only a new factorisation.
#[test]
fn releases_drive_analysis_and_factorisation() {
    let mut k = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
    for (i, j, v) in [
        (0, 0, 4.0),
        (0, 1, 1.0),
        (1, 1, 3.0),
        (1, 2, 1.0),
        (2, 2, 2.0),
    ] {
        k.add(i, j, v).unwrap();
    }
    let before = (k.profile_release(), k.matrix_type());
    k.promote_to_unsymmetric();
    assert_ne!(before, (k.profile_release(), k.matrix_type()));
    let mut s = EigenSparseLuSolver::default();
    // a factorisation against a stale analysis is refused
    let k0 = SparseMatrixAij::new("K0", 3, MatrixType::Symmetric);
    s.analysis(&k0).unwrap();
    assert!(s.factorize(&k).is_err());
    s.analysis(&k).unwrap();
    s.factorize(&k).unwrap();
    let mut x = [0.0; 3];
    s.solve(&[5.0, 5.0, 3.0], &mut x).unwrap();
    for (xi, e) in x.iter().zip([1.0, 1.0, 1.0]) {
        assert!((xi - e).abs() < 1e-14);
    }
}

/// The corrected `dget02` (no `n`) on hand-computable
/// cases, its zero conventions and its scale invariance.
#[test]
fn dget02_is_lapack_exactly() {
    // A = diag(2, 4), x = (1, 1), b = (2, 4 + 2ε·4): r = (0, −8ε) exactly
    // representable, ‖r‖₁ = 8ε, ‖A‖₁ = 4, ‖x‖₁ = 2 → ratio = 8ε/(4·2·ε) = 1
    // (the former `n` factor would have given 0.5).
    let a = CscMatrix::from_triplets(2, &[(0, 0, 2.0), (1, 1, 4.0)]).unwrap();
    let b = [2.0, 4.0 + 8.0 * EPS];
    assert_eq!(dget02(&a, &[1.0, 1.0], &b), 1.0);
    // zero residual → 0, including the zero system and a zero solution
    assert_eq!(dget02(&a, &[1.0, 1.0], &[2.0, 4.0]), 0.0);
    let z = CscMatrix::from_triplets(2, &[(0, 0, 0.0)]).unwrap();
    assert_eq!(dget02(&z, &[0.0, 0.0], &[0.0, 0.0]), 0.0);
    assert_eq!(dget02(&a, &[0.0, 0.0], &[0.0, 0.0]), 0.0);
    // nonzero residual with x = 0 or A = 0: LAPACK's 1/ε
    assert_eq!(dget02(&a, &[0.0, 0.0], &[1.0, 0.0]), 1.0 / EPS);
    assert_eq!(dget02(&z, &[1.0, 0.0], &[1.0, 0.0]), 1.0 / EPS);
    // scale-safe: the same relative residual at 2^±1000 gives the same
    // ratio (products ‖A‖‖x‖ε would underflow/overflow)
    for s in [2f64.powi(-1000), 2f64.powi(1000)] {
        let a_s = CscMatrix::from_triplets(2, &[(0, 0, 2.0 * s), (1, 1, 4.0 * s)]).unwrap();
        let b_s = [2.0 * s, (4.0 + 8.0 * EPS) * s];
        assert_eq!(dget02(&a_s, &[1.0, 1.0], &b_s), 1.0, "scale {s:e}");
    }
}

/// Zero right-hand side (zero reference solution) and a cancellation case:
/// both backends return the exact zero solution for `b = 0`, and recover a
/// manufactured solution whose right-hand side cancels almost completely.
#[test]
fn zero_rhs_and_cancellation_cases() {
    // nonsymmetric, needs pivoting
    let t = [
        (0, 0, 1e-3),
        (0, 1, 2.0),
        (1, 0, 3.0),
        (1, 1, 1.0),
        (1, 2, -1.0),
        (2, 1, 0.5),
        (2, 2, 4.0),
    ];
    let m = aij_from_triplets(3, &t);
    let a = CscMatrix::from_aij(&m).unwrap();
    for backend in ["eigen", "faer"] {
        let mut s: Box<dyn SparseDirectSolver> = if backend == "eigen" {
            Box::new(EigenSparseLuSolver::default())
        } else {
            Box::new(FaerLuSolver::default())
        };
        s.analysis(&m).unwrap();
        s.factorize(&m).unwrap();
        let mut x = [9.0; 3];
        s.solve(&[0.0; 3], &mut x).unwrap();
        assert!(x.iter().all(|&v| v == 0.0), "{backend}: {x:?}");
        assert_eq!(dget02(&a, &x, &[0.0; 3]), 0.0);
        // cancellation: rows of A·x* sum terms of opposite sign
        let xs = [1.0, -1.5e-3, -1.0];
        let b = a.mul_vec(&xs).unwrap();
        let mut x = [0.0; 3];
        s.solve(&b, &mut x).unwrap();
        let ratio = dget02(&a, &x, &b);
        let e = eta(&a, &x, &b);
        let kap = kappa_inf(&a, s.as_mut());
        let gamma3 = 3.0 * EPS / 2.0 / (1.0 - 3.0 * EPS / 2.0);
        let diff: Vec<f64> = x.iter().zip(&xs).map(|(p, q)| p - q).collect();
        let fwd = norm_inf(&diff) / norm_inf(&xs);
        let bound = forward_bound(kap, e, gamma3).unwrap() + FWD_ROUNDING_ALLOWANCE;
        assert!(ratio < DGET02_THRESH, "{backend}: ratio {ratio}");
        assert!(fwd <= bound, "{backend}: fwd {fwd:e} > {bound:e}");
        eprintln!(
            "{backend} cancellation: b = {b:?} kappa {kap:.2e} dget02 {ratio:.3} fwd {fwd:.2e} bound {bound:.2e}"
        );
    }
}
