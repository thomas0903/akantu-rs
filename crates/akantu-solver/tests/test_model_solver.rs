// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS):
// test/test_model/test_common/test_model_solver/{test_model_solver.cc,
// test_model_solver_dynamic.cc, test_dof_manager_default.cc} (+ .verified
// outputs) and the extended toy cases of
// oracle/probes/solver_model/solver_model_probe.cc.

//! Differential tests of the solver stack (DOF manager, Newton–Raphson /
//! linear / lumped solvers, time-step solver, every integration scheme)
//! against the oracle histories of `fixtures/solver_model/*`, with the
//! bit-identical Eigen `SparseLU` translation: iteration counts, errors,
//! energies, per-step SHA-256 digests of `u‖u̇‖ü`, snapshots, matrices
//! (COO, releases), residuals and callback traces are compared bit for bit;
//! the printed outputs are compared with upstream's `.verified` files
//! (`diff -biw`, the upstream test criterion).

mod common;

use akantu_comm::Communicator;
use akantu_core::Array;
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_linalg::matrix_market::MatrixMarketOptions;
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback};
use akantu_validation::Fixture;

use common::drivers::{run_dynamic, run_nl_static};
use common::my_model::{MyModel, gen_mesh};
use common::oracle::*;

/// Compare a dynamic run with its fixture; returns the number of compared
/// values.
fn check_dynamic(f: &Fixture) -> usize {
    let cid = f.case_id().to_owned();
    let run = run_dynamic(f, SparseSolverType::Eigen);
    let mut n = 0;
    if f.manifest().expect.status == akantu_validation::manifest::ExpectedStatus::Error {
        let step = param(f, "error_step").as_u64().unwrap() as usize;
        let (got_step, err) = run
            .failure
            .as_ref()
            .unwrap_or_else(|| panic!("{cid}: expected an error"));
        assert_eq!(*got_step, step, "{cid}: failing step");
        let msg = f
            .manifest()
            .expect
            .message_contains
            .clone()
            .unwrap_or_default();
        // upstream: "The matrix <id>:mtx:K does not exists in <id>"
        assert!(
            msg.contains("model_solver:dof_manager_default:mtx:K")
                && err
                    .to_string()
                    .contains("model_solver:dof_manager_default:mtx:K"),
            "{cid}: {err} vs upstream {msg}"
        );
        return 1;
    }
    assert!(run.failure.is_none(), "{cid}: {:?}", run.failure);
    n += assert_bits(f, "initial_residual", &run.initial_residual);
    n += assert_bits(f, "energies", &run.energies);
    if has(f, "newton/nb_iterations") {
        assert_eq!(
            run.n_iter,
            i64s(f, "newton/nb_iterations"),
            "{cid}: iterations"
        );
        n += assert_bits(f, "newton/error", &run.error);
        assert_eq!(
            run.converged,
            i64s(f, "newton/converged"),
            "{cid}: converged"
        );
    }
    assert_eq!(
        run.threw,
        i64s(f, "not_converged_exception"),
        "{cid}: exceptions"
    );
    let want: Vec<String> = param(f, "step_digests")
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap().to_owned())
        .collect();
    let first_bad = run.digests.iter().zip(&want).position(|(a, b)| a != b);
    assert!(
        run.digests.len() == want.len() && first_bad.is_none(),
        "{cid}: step digests differ from step {:?} ({} vs {} steps)",
        first_bad.map(|k| k + 1),
        run.digests.len(),
        want.len()
    );
    n += run.digests.len() * 3 * param(f, "nb_nodes").as_u64().unwrap() as usize;
    n += assert_bits(f, "snapshots/displacement", &run.u_snap);
    n += assert_bits(f, "snapshots/velocity", &run.v_snap);
    n += assert_bits(f, "snapshots/acceleration", &run.a_snap);
    assert_eq!(run.hooks, input_text(f, "hooks"), "{cid}: callback trace");
    let ms = run.solver.as_ref().unwrap();
    let dm = &ms.dof_manager;
    if has(f, "lumped_mass") {
        n += assert_bits(f, "lumped_mass", &dm.lumped_matrix("M").unwrap().values);
    } else {
        assert!(!dm.has_lumped_matrix("M"), "{cid}: lumped M");
    }
    for id in ["K", "M", "C", "J"] {
        let key = format!("matrix/{id}/nnz");
        if f.manifest().parameters.get(&key).is_some() {
            n += assert_matrix(f, &format!("matrix/{id}"), dm.matrix(id).unwrap());
        } else {
            assert!(!dm.has_matrix(id), "{cid}: matrix {id} exists only in Rust");
        }
    }
    n
}

/// Upstream printing of the dynamic tests (`std::scientific`, width 14).
fn dynamic_print(energies: &[f64], dt: f64) -> String {
    let w = |s: &str| setw(s, 14);
    let mut out = format!(
        "{},{},{},{},{},{},{}\n",
        w("time"),
        w("wext"),
        w("epot"),
        w("ekin"),
        w("total"),
        w("max_disp"),
        w("min_disp")
    );
    for (i, e) in energies.chunks(6).enumerate() {
        let t = if i == 0 { 0.0 } else { dt * i as f64 };
        let sci = |x: f64| akantu_solver::format::c_scientific(x, 6);
        out += &format!(
            "{},{},{},{},{},{},{}\n",
            w(&sci(t)),
            w(&sci(e[0])),
            w(&sci(e[1])),
            w(&sci(e[2])),
            w(&sci(e[3])),
            w(&sci(e[4])),
            w(&sci(e[5]))
        );
    }
    out
}

/// `test_model_solver_dynamic_explicit` (upstream criterion: the printed
/// table equals the `.verified` file) + bitwise oracle histories.
#[test]
fn test_model_solver_dynamic_explicit() {
    let f = case("dynamic_explicit");
    let n = check_dynamic(&f);
    let run = run_dynamic(&f, SparseSolverType::Eigen);
    let printed = dynamic_print(&run.energies, 0.001);
    assert!(
        same_text_ignoring_whitespace(&printed, &input_text(&f, "verified")),
        "printed table differs from test_model_solver_dynamic_explicit.verified"
    );
    println!("dynamic_explicit: {n} values bit-identical, .verified text identical");
}

/// `test_model_solver_dynamic_implicit` with the Eigen solver: bitwise vs
/// the oracle's Eigen run; the `.verified` file (MUMPS) is matched as text.
#[test]
fn test_model_solver_dynamic_implicit() {
    let f = case("dynamic_implicit");
    let n = check_dynamic(&f);
    let run = run_dynamic(&f, SparseSolverType::Eigen);
    let printed = dynamic_print(&run.energies, 0.001);
    let verified = input_text(&f, "verified");
    let same = same_text_ignoring_whitespace(&printed, &verified);
    let differing: Vec<usize> = printed
        .lines()
        .zip(verified.lines())
        .enumerate()
        .filter(|(_, (a, b))| {
            a.split_whitespace().collect::<String>() != b.split_whitespace().collect::<String>()
        })
        .map(|(i, _)| i)
        .collect();
    assert!(
        same,
        "printed table differs from test_model_solver_dynamic_implicit.verified (MUMPS) at lines {differing:?}"
    );
    println!("dynamic_implicit: {n} values bit-identical, .verified text identical");
}

/// Every integration scheme / solution type variant of the probe.
#[test]
fn integration_scheme_variants_match_oracle() {
    let mut total = 0;
    let mut count = 0;
    for f in cases() {
        if !f.case_id().starts_with("solver_model/scheme_")
            && !f.case_id().ends_with("nl_dynamic_not_converged_restore")
        {
            continue;
        }
        let n = check_dynamic(&f);
        println!("{}: {n} values bit-identical", f.case_id());
        total += n;
        count += 1;
    }
    assert_eq!(count, 16);
    println!("scheme variants: {count} cases, {total} values bit-identical");
}

/// TS-004, TS-005, TS-006: the probe's `DampedBar`
/// (Rayleigh `C = 0.5 M + 0.005 K`) on the dynamic driver — the
/// second-order residual `r −= C·u̇`, the Newmark Jacobian term `d·C`, the
/// central-difference `J = M + Δt/2·C`, and the lumped central difference
/// (which creates `C` at `set_integration_scheme` and never assembles it)
/// replayed bit for bit (x86_64 oracle, solver_model probe v3).
#[test]
fn damped_variants_match_oracle() {
    let mut total = 0;
    let mut count = 0;
    for f in cases() {
        if !f.case_id().starts_with("solver_model/damped_") {
            continue;
        }
        assert!(f.manifest().parameters.get("rayleigh").is_some());
        let n = check_dynamic(&f);
        println!("{}: {n} values bit-identical", f.case_id());
        total += n;
        count += 1;
    }
    assert_eq!(count, 4);
    println!("damped variants: {count} cases, {total} values bit-identical");
}

/// `test_model_solver` (static Newton–Raphson, pseudo-time,
/// `max_iterations = 2`) with Eigen; printed output equal to the
/// `.verified` file (MUMPS).
#[test]
fn test_model_solver_static() {
    let f = case("static_model_solver");
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
    let nls = ms.non_linear_solver(None).unwrap();
    assert_eq!(
        nls.nb_iterations() as i64,
        param(&f, "nb_iterations").as_i64().unwrap()
    );
    assert_eq!(nls.converged(), param(&f, "converged").as_bool().unwrap());
    let mut n = assert_bits(&f, "newton/error", &[nls.error()]);
    let dm = &ms.dof_manager;
    n += assert_bits(&f, "displacement", dm.dofs("disp").unwrap().as_slice());
    n += assert_bits(&f, "residual", &dm.residual().values());
    n += assert_bits(&f, "solution", dm.solution());
    n += assert_matrix(&f, "matrix/K", dm.matrix("K").unwrap());
    n += assert_matrix(&f, "matrix/J", dm.matrix("J").unwrap());
    assert_eq!(
        model.trace_text(),
        input_text(&f, "hooks"),
        "callback trace"
    );
    // printResults (setprecision(7), setw(8))
    let g = |x: f64| akantu_solver::format::c_general(x, 7);
    let mut out = format!(
        "node, {}, {}, {}\n",
        setw("disp", 8),
        setw("force", 8),
        setw("blocked", 8)
    );
    let u = dm.dofs("disp").unwrap();
    let u = u.as_slice();
    let b = dm.blocked_dofs("disp").unwrap();
    let b = b.as_slice();
    for i in 0..100 {
        out += &format!(
            "{i}, {}, {}, {}\n",
            setw(&g(u[i]), 8),
            setw(&g(model.forces[i]), 8),
            setw(if b[i] { "1" } else { "0" }, 8)
        );
    }
    assert_eq!(out, input_text(&f, "print_results"), "printResults text");
    assert!(
        same_text_ignoring_whitespace(&out, &input_text(&f, "verified")),
        "printResults differs from test_model_solver_mumps.verified"
    );
    println!("static_model_solver: {n} values bit-identical, .verified text identical");
}

/// The probe's non-linear static cases (Newton–Raphson variants,
/// convergence criteria, linear solver, non-convergence with restore,
/// residual criterion converged at the first test — UD-010 legacy hooks).
#[test]
fn non_linear_solver_variants_match_oracle() {
    let mut count = 0;
    let mut total = 0;
    for f in cases() {
        let cid = f.case_id().to_owned();
        if !cid.starts_with("solver_model/nl_") || cid.contains("dynamic") {
            continue;
        }
        let run = run_nl_static(&f, SparseSolverType::Eigen, true);
        let mut n = 0;
        assert_eq!(
            run.threw,
            i64s(&f, "not_converged_exception"),
            "{cid}: exceptions"
        );
        if has(&f, "newton/nb_iterations") {
            assert_eq!(
                run.n_iter,
                i64s(&f, "newton/nb_iterations"),
                "{cid}: iterations"
            );
            n += assert_bits(&f, "newton/error", &run.error);
            assert_eq!(
                run.converged,
                i64s(&f, "newton/converged"),
                "{cid}: converged"
            );
        }
        n += assert_bits(&f, "displacement", &run.u);
        n += assert_bits(&f, "residual", &run.r);
        assert_eq!(
            run.hooks,
            input_text(&f, "hooks"),
            "{cid}: callback trace (legacy hooks)"
        );
        n += assert_matrix(
            &f,
            "matrix/J",
            run.solver
                .as_ref()
                .unwrap()
                .dof_manager
                .matrix("J")
                .unwrap(),
        );
        println!(
            "{cid}: {n} values bit-identical, iterations {:?}",
            run.n_iter
        );
        total += n;
        count += 1;
    }
    assert_eq!(count, 8);
    println!("non-linear static: {count} cases, {total} values bit-identical");
}

/// UD-010 (default, fixed): the step converged at the initial residual test
/// completes the successful-step lifecycle with `after_solve_step(true)`;
/// everything else is identical to upstream's legacy trace.
#[test]
fn ud010_successful_step_lifecycle_completes() {
    let f = case("nl_residual_converged_at_first_test");
    let fixed = run_nl_static(&f, SparseSolverType::Eigen, false);
    let legacy = run_nl_static(&f, SparseSolverType::Eigen, true);
    assert_eq!(fixed.u, legacy.u);
    assert_eq!(fixed.n_iter, legacy.n_iter);
    let upstream = input_text(&f, "hooks");
    assert!(
        !upstream.ends_with("after_solve_step:true\n"),
        "upstream misses the hook"
    );
    assert_eq!(fixed.hooks, format!("{upstream}after_solve_step:true\n"));
}

/// `test_dof_manager_default` (generic DOFs, linear solver, pseudo-time)
/// with Eigen: matrices, `saveMatrix` bytes, displacements and the printed
/// table vs the `.verified` file.
#[test]
fn test_dof_manager_default() {
    struct MySolverCallback {
        forces: Vec<f64>,
        nb_dofs: usize,
    }
    impl SolverCallback for MySolverCallback {
        fn matrix_type(&self, id: &str) -> MatrixType {
            if id == "K" {
                MatrixType::Symmetric
            } else {
                MatrixType::NotDefined
            }
        }
        fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> akantu_core::Result<()> {
            if id != "K" {
                return Ok(());
            }
            let n = self.nb_dofs as i64;
            let k = dm.matrix_mut("K")?;
            k.zero();
            for i in 1..n - 1 {
                k.add(i, i, 2.0)?;
            }
            for i in 0..n - 1 {
                k.add(i, i + 1, -1.0)?;
            }
            k.add(0, 0, 1.0)?;
            k.add(n - 1, n - 1, 1.0)?;
            k.mul((n - 1) as f64);
            Ok(())
        }
        fn assemble_lumped_matrix(
            &mut self,
            _: &mut DofManager,
            _: &str,
        ) -> akantu_core::Result<()> {
            Ok(())
        }
        fn assemble_residual(&mut self, dm: &mut DofManager) -> akantu_core::Result<()> {
            let fa = Array::from_vec(self.forces.clone(), 1)?;
            dm.assemble_to_residual("disp", fa.view(), 1.0)
        }
    }
    let f = case("dof_manager_default");
    let nb = 11;
    let mut dm = DofManager::new("test_dof_manager", Communicator::serial());
    dm.register_generic_dofs("disp", nb, 1).unwrap();
    dm.register_blocked_dofs("disp").unwrap();
    dm.blocked_dofs_mut("disp").unwrap().as_mut_slice()[0] = true;
    let mut cb = MySolverCallback {
        forces: (0..nb)
            .map(|i| if i == nb - 1 { 10.0 } else { 0.0 })
            .collect(),
        nb_dofs: nb,
    };
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::Linear),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.new_solver("my_tss", &opts).unwrap();
    ms.set_integration_scheme(
        "my_tss",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &cb,
    )
    .unwrap();
    ms.solve_step(&mut cb, None).unwrap();
    let dm = &ms.dof_manager;
    let mut n = assert_bits(&f, "displacement", dm.dofs("disp").unwrap().as_slice());
    n += assert_bits(&f, "residual", &dm.residual().values());
    n += assert_matrix(&f, "matrix/K", dm.matrix("K").unwrap());
    n += assert_matrix(&f, "matrix/J", dm.matrix("J").unwrap());
    assert_eq!(
        // upstream `saveMatrix` writes `digits10` = 15 digits (UD-061; the
        // port's default is 17): byte comparison with the upstream variant
        dm.matrix("K")
            .unwrap()
            .matrix_market_text_with(&MatrixMarketOptions::upstream_precision()),
        input_text(&f, "save_matrix_K"),
        "saveMatrix bytes"
    );
    let g = |x: f64| akantu_solver::format::c_general(x, 6);
    let mut out = format!(
        "{} {} {}\n",
        setw("disp", 8),
        setw("force", 8),
        setw("blocked", 8)
    );
    let u = dm.dofs("disp").unwrap();
    let u = u.as_slice();
    let b = dm.blocked_dofs("disp").unwrap();
    let b = b.as_slice();
    for i in 0..nb {
        out += &format!(
            "{} {} {}\n",
            setw(&g(u[i]), 8),
            setw(&g(cb.forces[i]), 8),
            setw(if b[i] { "true" } else { "false" }, 8)
        );
    }
    assert_eq!(out, input_text(&f, "print_results"));
    assert!(same_text_ignoring_whitespace(
        &out,
        &input_text(&f, "verified")
    ));
    println!(
        "dof_manager_default: {n} values bit-identical, saveMatrix bytes and .verified text identical"
    );
}
