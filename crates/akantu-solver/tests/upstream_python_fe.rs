// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/python_fe/{fe.py, truss_fe.py,
// static_solver.py} (independent NumPy truss model and static solver) and
// test/test_model/test_common/test_model_solver/test_model_solver.py (the
// check of the static `test_model_solver` result).

//! GOLD-010 / TEST-232: upstream's independent Python oracle of
//! `test_model_solver` (TEST-073), translated.
//!
//! `test_model_solver.py` reads the solved bar (`node, disp, force, blocked`
//! per node, the printout of `printResults`), rebuilds the problem with the
//! `python_fe` truss model (`TrussFE`: `E = A = L = 1`, `Ne` elements of
//! length `L / Ne`, `Ke = EA/Le [[1, −1], [−1, 1]]`, the printed forces as
//! nodal loads, the blocked nodes with their printed displacements), solves
//! it with `StaticSolver` (`J = K` with the rows of the blocked DOFs zeroed
//! except their diagonal, `r = f_ext − K u`, `u += J⁻¹ r`, then the Dirichlet
//! values re-applied) and fails if `‖u_py − u‖ / ‖u_py‖ > 1e-14`.
//!
//! Upstream's script is dead (UD-699), a stale check: it was written in
//! 2016 for an 11-node bar with `F = +10` whose displacements are the
//! integers 0…10, read from a `disp.csv` that the C++ test wrote at default
//! (6-digit) precision from 2016-12-02 (`cafceb591`) to 2017-04-01
//! (`a62caba66`); the driver moved to 100 nodes on 2017-05-30 (`dd2df4431`),
//! after the writer had gone, so the check has not run since 2017. It is not
//! registered, `python_fe/__init__.py` does not import the sub-modules it
//! uses, and NumPy (2.2.4, verified) rejects its `np.where(…)` tuples as row
//! indices. Run with those things fixed on the **oracle's** full-precision
//! displacement (`oracle/probes/gold/python_fe_check.sh`, x86_64 oracle
//! image, NumPy 2.2.4 / SciPy 1.15.3: `fixtures/gold/test_model_solver_py/`),
//! it reports a relative error of 1.19e-14 and **fails** its own `1e-14`
//! criterion: with 99 elements the two computations differ by rounding
//! (element lengths `L / Ne` against differences of rounded node
//! coordinates, SuperLU against Eigen's `SparseLU`), amplified by
//! `cond(K) ≈ 16 Ne² / π² ≈ 1.6e4`, which is more than the criterion allows;
//! in the 2016 configuration (exact integer answer) it passes
//! (2.19e-15).
//!
//! This test reproduces that check on the port and compares it the way it
//! would have been used:
//!
//! 1. the input — the port's static solve of the same model (`MyModel`,
//!    100 nodes, `F = −10` at the tip, node 0 blocked, Newton–Raphson with at
//!    most 2 iterations, pseudo-time) — is **bit-identical** to the oracle's
//!    displacement that upstream's script was run on;
//! 2. the `python_fe` truss model and static solve are translated (dense
//!    Gaussian elimination with partial pivoting in place of SuperLU, so the
//!    rounding differs from SciPy's) and give the same **verdict** as
//!    upstream's run: the relative error exceeds `1e-14` (rule A-upstream:
//!    the criterion is kept, not loosened; its defect is UD-699).
//!
//! Assertion 1 is the check of the port (it duplicates the bitwise
//! `test_model_solver::test_model_solver_static`). Assertion 2 compares a
//! verdict computed by test-local code (the translated Python reference and
//! its dense solve): it documents UD-699 (upstream's criterion fails on
//! upstream's own result), not the port's correctness. Its margin is small
//! (1.05e-14 against 1e-14) but the verdict does not depend on the solver of
//! the reference: a Thomas solve gives 1.08e-14 and
//! upstream's SuperLU gives 1.19e-14, all failures.

mod common;

use akantu_comm::Communicator;
use akantu_core::Real;
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};

use common::my_model::{MyModel, gen_mesh};
use common::oracle::{assert_bits, case};

/// The solved bar: `(disp, force, blocked)` per node (the columns of
/// `printResults`, full precision).
fn solve_test_model_solver() -> (Vec<Real>, Vec<Real>, Vec<bool>) {
    let mut dm = DofManager::new("model_solver:dof_manager_default", Communicator::serial());
    let mut model = MyModel::new(-10.0, gen_mesh(100), false, &mut dm).unwrap();
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("static", &opts, &mut model).unwrap();
    ms.set_integration_scheme(
        "static",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &model,
    )
    .unwrap();
    ms.non_linear_solver_mut(Some("static"))
        .unwrap()
        .set_param("max_iterations", ParamValue::Int(2))
        .unwrap();
    ms.solve_step(&mut model, None).unwrap();
    let dm = &ms.dof_manager;
    (
        dm.dofs("disp").unwrap().as_slice().to_vec(),
        model.forces.clone(),
        dm.blocked_dofs("disp").unwrap().as_slice().to_vec(),
    )
}

/// `python_fe.TrussFE` (`E = A = L = 1`) with `StaticSolver.solveStep`:
/// returns `trusses.u` (Dirichlet values applied).
fn python_fe_static(ne: usize, force: &[Real], blocked: &[(usize, Real)]) -> Vec<Real> {
    let (e, a, l) = (1.0, 1.0, 1.0);
    let le = l / ne as Real;
    let n = ne + 1;
    // assembleStiffness: Ke *= E A / Le, K[i:i+2, i:i+2] += Ke
    let mut k = vec![vec![0.0; n]; n];
    let ke = e * a / le;
    for i in 0..ne {
        k[i][i] += ke;
        k[i][i + 1] -= ke;
        k[i + 1][i] -= ke;
        k[i + 1][i + 1] += ke;
    }
    // applyNeumannBC: F_ext[nodes] = force; applyDirichletBC: u[b] = value
    let f_ext = force.to_vec();
    let mut u = vec![0.0; n];
    for &(b, v) in blocked {
        u[b] = v;
    }
    // _assembleJacobian: J = K, rows of the blocked DOFs zeroed but the
    // diagonal
    let mut jac = k.clone();
    for &(b, _) in blocked {
        let diag = jac[b][b];
        jac[b].iter_mut().for_each(|x| *x = 0.0);
        jac[b][b] = diag;
    }
    // _assembleResidual: r = f_ext − K u
    let r: Vec<Real> = (0..n)
        .map(|i| f_ext[i] - (0..n).map(|j| k[i][j] * u[j]).sum::<Real>())
        .collect();
    // spsolve(J, r): Gaussian elimination with partial pivoting
    let x = dense_solve(jac, r);
    for i in 0..n {
        u[i] += x[i];
    }
    // the `u` property re-applies the Dirichlet values
    for &(b, v) in blocked {
        u[b] = v;
    }
    u
}

fn dense_solve(mut a: Vec<Vec<Real>>, mut b: Vec<Real>) -> Vec<Real> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n)
            .max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))
            .unwrap();
        a.swap(c, p);
        b.swap(c, p);
        assert!(a[c][c] != 0.0, "singular Jacobian");
        for r in c + 1..n {
            let m = a[r][c] / a[c][c];
            if m != 0.0 {
                let (top, bottom) = a.split_at_mut(r);
                for (x, p) in bottom[0][c..].iter_mut().zip(&top[c][c..]) {
                    *x -= m * p;
                }
                b[r] -= m * b[c];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: Real = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    x
}

fn norm(v: impl Iterator<Item = Real>) -> Real {
    v.map(|x| x * x).sum::<Real>().sqrt()
}

/// Upstream's run on the oracle's output: `(relative error, exit status)`.
fn upstream_run() -> (Real, i32) {
    let d = akantu_validation::fixtures_root().join("gold/test_model_solver_py");
    let out = std::fs::read_to_string(d.join("output.txt")).unwrap();
    let line = out
        .lines()
        .find(|l| l.starts_with("relative error"))
        .expect("relative error line");
    // `relative error np.float64(1.1916780266424304e-14)` (repr: round trip)
    let v = line
        .trim_start_matches("relative error")
        .trim()
        .trim_start_matches("np.float64(")
        .trim_end_matches(')');
    let status = std::fs::read_to_string(d.join("exit_status")).unwrap();
    (v.parse().unwrap(), status.trim().parse().unwrap())
}

#[test]
fn test_model_solver_python_fe_check() {
    let (u, force, blocked) = solve_test_model_solver();
    // 1. the script's input: the oracle's displacement, bit for bit
    assert_bits(&case("static_model_solver"), "displacement", &u);
    // `Ne = int(sim_u["node"][-1])`
    let ne = u.len() - 1;
    let blocked: Vec<(usize, Real)> = blocked
        .iter()
        .enumerate()
        .filter(|(_, b)| **b)
        .map(|(i, _)| (i, u[i]))
        .collect();
    assert_eq!(blocked, vec![(0, 0.0)], "node 0 blocked at u = 0");
    // 2. `python_fe`: `n = ||u_py - u|| / ||u_py||`, failure if `n > 1e-14`
    let upy = python_fe_static(ne, &force, &blocked);
    let n = norm(upy.iter().zip(&u).map(|(p, q)| p - q)) / norm(upy.iter().copied());
    let (n_upstream, status_upstream) = upstream_run();
    println!(
        "test_model_solver.py: ||u_py - u|| / ||u_py|| = {n:e} (translation, port output) vs \
         {n_upstream:e} (upstream script, SciPy, oracle output); criterion 1e-14; tip u = {}",
        u[ne]
    );
    let fails = n > 1e-14;
    assert_eq!(
        fails,
        status_upstream != 0,
        "verdict of the python_fe check differs from upstream's run (UD-699)"
    );
    assert_eq!(n_upstream > 1e-14, status_upstream != 0, "upstream record");
}
