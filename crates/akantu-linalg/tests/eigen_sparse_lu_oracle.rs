// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the intent of
// test/test_solver/test_sparse_solver_eigen.cc (SparseSolverEigen on a
// preassembled matrix, solution checked) extended to the upstream meshes and
// to LU edge cases, compared bit for bit with fixtures/solver/{aij,lu}/*
// (oracle: Eigen 3.4.0 SparseLU in the strict build, including its internal
// factor arrays).

//! Differential tests of the Eigen `SparseLU` translation (SOLV-004).

mod solver_oracle;

use akantu_core::exec::{Exec, ExecPolicy, Grain};
use akantu_linalg::direct::{CscMatrix, SparseDirectSolver};
use akantu_linalg::eigen_sparse_lu::{EigenSparseLuSolver, SparseLu};
use akantu_validation::Fixture;
use solver_oracle::*;

fn check_ints(f: &Fixture, name: &str, got: &[i64]) {
    let want = i64s(f, name);
    assert_eq!(got, want.as_slice(), "{}: {name}", f.case_id());
}

fn check_reals(f: &Fixture, name: &str, got: &[f64]) {
    let want = f64s(f, name);
    let (n, worst) = diff_stats(got, &want);
    assert!(
        same_bits(got, &want),
        "{}: {name}: {n} of {} differ (max rel {worst:e})",
        f.case_id(),
        want.len()
    );
}

/// The executors of the thread-invariance runs: 2, 3, 4 and 8 threads,
/// automatic and finest grain (D-123).
fn threaded_execs() -> Vec<Exec> {
    let mut v = Vec::new();
    for threads in [2, 3, 4, 8] {
        for grain in [Grain::Auto, Grain::Finest] {
            let policy = ExecPolicy::serial()
                .with_threads(threads)
                .unwrap()
                .with_grain(grain);
            v.push(Exec::new(policy).unwrap());
        }
    }
    v
}

/// Analyse, factorise and solve `a`, comparing everything the probe stored
/// under `lu/`. Returns the number of compared values.
fn check_lu(f: &Fixture, a: &CscMatrix, b: &[f64]) -> usize {
    let mut n = check_lu_with(f, a, b, None);
    // the same bits with every panel update on threads
    for exec in threaded_execs() {
        n += check_lu_with(f, a, b, Some(exec));
    }
    n
}

fn check_lu_with(f: &Fixture, a: &CscMatrix, b: &[f64], exec: Option<Exec>) -> usize {
    let mut lu = SparseLu::default();
    if let Some(exec) = exec {
        lu.set_exec(exec);
        lu.set_threaded_update_min(Some(0));
    }
    lu.analyze_pattern(a).unwrap();
    check_ints(f, "lu/perm_c", lu.perm_c());
    check_ints(f, "lu/etree", lu.etree());
    let mut compared = 2 * a.n();
    let ok = param(f, "lu_info_success").as_bool().unwrap();
    let res = lu.factorize(a);
    if !ok {
        let err = res.expect_err("the oracle's factorisation failed");
        let want = param_str(f, "lu_last_error");
        assert!(
            err.to_string().contains(want.trim()),
            "{}: {err} vs {want}",
            f.case_id()
        );
        return compared;
    }
    res.unwrap();
    let fac = lu.factors().unwrap();
    check_ints(f, "lu/perm_r", &fac.perm_r);
    assert_eq!(fac.nnz_l, scalar_i(f, "lu/nnz_l"), "{}: nnzL", f.case_id());
    assert_eq!(fac.nnz_u, scalar_i(f, "lu/nnz_u"), "{}: nnzU", f.case_id());
    if f.quantity("lu/lusup").is_ok() {
        check_ints(f, "lu/xsup", &fac.xsup);
        check_ints(f, "lu/supno", &fac.supno);
        check_ints(f, "lu/xlsub", &fac.xlsub);
        check_ints(f, "lu/lsub", &fac.lsub);
        check_ints(f, "lu/xlusup", &fac.xlusup);
        check_reals(f, "lu/lusup", &fac.lusup);
        check_ints(f, "lu/xusub", &fac.xusub);
        check_ints(f, "lu/usub", &fac.usub);
        check_reals(f, "lu/ucol", &fac.ucol);
        compared += fac.lusup.len() + fac.ucol.len();
    } else {
        assert_eq!(
            fac.xsup.len() as i64 - 2,
            param(f, "lu_nsuper").as_i64().unwrap(),
            "{}: nsuper",
            f.case_id()
        );
    }
    let mut x = vec![0.0; a.n()];
    lu.solve(b, &mut x).unwrap();
    check_reals(f, "lu/x", &x);
    compared + a.n()
}

#[test]
fn sparse_lu_edge_cases_match_oracle_bitwise() {
    let mut total = 0;
    for f in cases("lu") {
        let n = usize::try_from(param(&f, "n").as_i64().unwrap()).unwrap();
        let (r, c, v) = (i64s(&f, "A/rows"), i64s(&f, "A/cols"), f64s(&f, "A/values"));
        let t: Vec<(usize, usize, f64)> = r
            .iter()
            .zip(&c)
            .zip(&v)
            .map(|((&i, &j), &x)| (i as usize, j as usize, x))
            .collect();
        let a = CscMatrix::from_triplets(n, &t).unwrap();
        let k = check_lu(&f, &a, &f64s(&f, "b"));
        eprintln!("{}: {k} values bit-identical", f.case_id());
        total += k;
    }
    assert!(total > 0);
}

#[test]
fn sparse_solver_eigen_on_fe_matrices_matches_oracle_bitwise() {
    for f in cases("aij") {
        let k = replay_k_bc(&f);
        let a = CscMatrix::from_aij(&k).unwrap();
        let b = f64s(&f, "eigen/b");
        let n = check_lu(&f, &a, &b);
        // the backend path (SparseSolverEigen): same solution
        let mut s = EigenSparseLuSolver::default();
        s.analysis(&k).unwrap();
        s.factorize(&k).unwrap();
        let mut x = vec![0.0; b.len()];
        s.solve(&b, &mut x).unwrap();
        check_reals(&f, "eigen/x", &x);
        eprintln!("{}: {n} values bit-identical", f.case_id());
    }
}
