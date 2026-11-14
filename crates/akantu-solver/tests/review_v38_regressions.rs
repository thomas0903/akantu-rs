// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests of the solver stack
// ported from Akantu (EPFL/LSMS): src/model/model_solver.cc
// (initDOFManager), src/model/common/{integration_scheme/
// {pseudo_time.cc, generalized_trapezoidal.cc, newmark-beta.cc},
// time_step_solvers/time_step_solver_default.cc, non_linear_solver/
// non_linear_solver_newton_raphson.cc}, on the upstream MyModel toy
// (test/test_model/test_common/test_model_solver/test_model_solver_my_model.hh).

//! Regressions:
//!
//! * `sv01_*`: a `.dat` integration scheme without `solution_type` gets
//!   upstream's value-initialised `_displacement`;
//! * `sv03_*` / UD-193: static and dynamic solvers sharing one DOF manager —
//!   oracle fixture `solver_model/solver_switch_shared_j` replayed bit for
//!   bit with `UpstreamCompat::stale_shared_jacobian`, and the corrected
//!   default converging, every step equal (bitwise) to the same solver
//!   running alone from the same state;
//! * `sv04_*` / UD-194: `residual_mass_wgh` refused by default;
//! * `sv05_*`: the UD-010 early-convergence path refreshes the increment;
//! * `sv06_*` / UD-195: forward Euler with a consistent mass solves (`J = M`)
//!   and follows the exact explicit recursion and first-order accuracy;
//! * `sv07_*`: family arrays resized through the public API are errors, not
//!   panics.

mod common;

use akantu_comm::Communicator;
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SolveConvergenceCriteria,
    SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Array, Error, Real};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::{
    DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback, UpstreamCompat,
};

use common::drivers::upstream_compat;
use common::my_model::{MyModel, gen_mesh};
use common::oracle::{assert_bits, case, i64s, param};

// ------------------------------------------------------------------

/// Upstream `initDOFManager(section)` reads `solver_options.solution_type
/// [dof_id]` (`std::map::operator[]`: value-initialised to 0 =
/// `_displacement`, checked on the oracle) for a scheme section without
/// `solution_type` and for the model's default schemes. Before the fix the
/// port stored `NotDefined` and the first step failed ("The corrector type
/// : -1 is not supported").
#[test]
fn sv01_scheme_section_without_solution_type_defaults_to_displacement() {
    let text = "model model_solver [
  time_step_solver dynamic [
    sparse_solver_type = eigen
    integration_scheme trapezoidal_rule_2 [
      name = disp
    ]
  ]
]
";
    let parser = Parser::parse_str(text, "solvers.dat").unwrap();
    let section = &parser.root.sub_sections[0];
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(-1.0, gen_mesh(11), false, &mut dm).unwrap();
    let mut ms = ModelSolver::new(dm);
    ms.init_from_section(section, &mut model, false).unwrap();
    assert_eq!(
        ms.solver(Some("dynamic")).unwrap().solution_type("disp"),
        Some(SolutionType::Displacement)
    );
    ms.set_time_step(0.01, Some("dynamic")).unwrap();
    ms.solve_step(&mut model, Some("dynamic")).unwrap();
    assert!(ms.non_linear_solver(Some("dynamic")).unwrap().converged());
}

// ------------------------------------------------------- / UD-193

/// Histories of the `solver_switch_shared_j` scenario.
#[derive(Debug, Default)]
struct SwitchRun {
    solvers: Vec<String>,
    n_iter: Vec<i64>,
    error: Vec<f64>,
    converged: Vec<i64>,
    threw: Vec<i64>,
    j_value: Vec<i64>,
    j_profile: Vec<i64>,
    u: Vec<f64>,
    v: Vec<f64>,
    a: Vec<f64>,
    r: Vec<f64>,
    /// Per solve: the state before it (`u`, `u̇`, `ü`, blocked, end force).
    before: Vec<State>,
}

#[derive(Clone, Debug)]
struct State {
    u: Vec<f64>,
    v: Vec<f64>,
    a: Vec<f64>,
    blocked: Vec<bool>,
    force: f64,
}

const SWITCH_NODES: usize = 21;
const SWITCH_DT: Real = 0.05;

fn switch_options(tss: TimeStepSolverType) -> ModelSolverOptions {
    ModelSolverOptions {
        time_step_solver_type: Some(tss),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    }
}

/// Add solver `id` (`static`: pseudo-time; `dynamic`: trapezoidal rule 2,
/// displacement) to `ms`, as the probe does.
fn add_switch_solver(ms: &mut ModelSolver, model: &mut MyModel, id: &str) {
    if id == "static" {
        ms.get_new_solver(id, &switch_options(TimeStepSolverType::Static), model)
            .unwrap();
        ms.set_integration_scheme(
            id,
            "disp",
            IntegrationSchemeType::PseudoTime,
            SolutionType::NotDefined,
            model,
        )
        .unwrap();
    } else {
        ms.get_new_solver(id, &switch_options(TimeStepSolverType::Dynamic), model)
            .unwrap();
        ms.set_integration_scheme(
            id,
            "disp",
            IntegrationSchemeType::TrapezoidalRule2,
            SolutionType::Displacement,
            model,
        )
        .unwrap();
        ms.set_time_step(SWITCH_DT, Some(id)).unwrap();
    }
}

fn state_of(ms: &ModelSolver, model: &MyModel) -> State {
    let dm = &ms.dof_manager;
    State {
        u: dm.dofs("disp").unwrap().as_slice().to_vec(),
        v: dm.dofs_derivative("disp", 1).unwrap().as_slice().to_vec(),
        a: dm.dofs_derivative("disp", 2).unwrap().as_slice().to_vec(),
        blocked: dm.blocked_dofs("disp").unwrap().as_slice().to_vec(),
        force: model.forces[SWITCH_NODES - 1],
    }
}

fn solve_counting(ms: &mut ModelSolver, model: &mut MyModel, id: &str) -> bool {
    match ms.solve_step(model, Some(id)) {
        Ok(()) => false,
        Err(Error::NotConverged { .. }) => true,
        Err(e) => panic!("solver {id}: {e}"),
    }
}

/// The probe's scenario (`solver_model_probe.cc::solver_switch_case`).
fn run_switch(tokens: &[String], compat: UpstreamCompat) -> SwitchRun {
    let n = SWITCH_NODES;
    let mut dm = DofManager::new("model_solver:dof_manager_default", Communicator::serial());
    dm.set_upstream_compat(compat);
    let mut model = MyModel::new(-1.0, gen_mesh(n), false, &mut dm).unwrap();
    let mut ms = ModelSolver::new(dm);
    add_switch_solver(&mut ms, &mut model, "static");
    add_switch_solver(&mut ms, &mut model, "dynamic");
    {
        let mut v = ms.dof_manager.dofs_derivative_mut("disp", 1).unwrap();
        for (i, x) in v.as_mut_slice().iter_mut().enumerate() {
            *x = 0.1 * i as f64;
        }
    }
    let mut run = SwitchRun::default();
    for tok in tokens {
        if let Some(f) = tok.strip_prefix("force=") {
            model.forces[n - 1] = f.parse().unwrap();
            continue;
        }
        if let Some(node) = tok.strip_prefix("block=") {
            let node: usize = node.parse().unwrap();
            ms.dof_manager
                .blocked_dofs_mut("disp")
                .unwrap()
                .as_mut_slice()[node] = true;
            continue;
        }
        run.before.push(state_of(&ms, &model));
        let threw = solve_counting(&mut ms, &mut model, tok);
        let nls = ms.non_linear_solver(Some(tok)).unwrap();
        run.solvers.push(tok.clone());
        run.threw.push(i64::from(threw));
        run.n_iter.push(nls.nb_iterations() as i64);
        run.error.push(nls.error());
        run.converged.push(i64::from(nls.converged()));
        let dm = &ms.dof_manager;
        let j = dm.matrix("J").unwrap();
        run.j_value.push(j.value_release() as i64);
        run.j_profile.push(j.profile_release() as i64);
        run.u.extend_from_slice(dm.dofs("disp").unwrap().as_slice());
        run.v
            .extend_from_slice(dm.dofs_derivative("disp", 1).unwrap().as_slice());
        run.a
            .extend_from_slice(dm.dofs_derivative("disp", 2).unwrap().as_slice());
        run.r.extend_from_slice(&dm.residual().values());
    }
    run
}

fn switch_tokens() -> Vec<String> {
    let f = case("solver_switch_shared_j");
    param(&f, "sequence")
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect()
}

/// Upstream behaviour (`stale_shared_jacobian`): bit for bit the oracle,
/// including the non-convergence at step 4 (static after dynamic steps,
/// 10 iterations, error 8.5e-3) and the divergence of steps 7 and 8
/// (dynamic after a static rebuild, error 8.4e18).
#[test]
fn sv03_solver_switch_shared_j_legacy_matches_oracle() {
    let f = case("solver_switch_shared_j");
    let run = run_switch(&switch_tokens(), upstream_compat());
    let want_solvers: Vec<String> = param(&f, "step_solvers")
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(run.solvers, want_solvers);
    assert_eq!(run.n_iter, i64s(&f, "newton/nb_iterations"));
    assert_eq!(run.converged, i64s(&f, "newton/converged"));
    assert_eq!(run.threw, i64s(&f, "not_converged_exception"));
    assert_eq!(run.j_value, i64s(&f, "jacobian/value_release"));
    assert_eq!(run.j_profile, i64s(&f, "jacobian/profile_release"));
    let mut n = assert_bits(&f, "newton/error", &run.error);
    n += assert_bits(&f, "displacement", &run.u);
    n += assert_bits(&f, "velocity", &run.v);
    n += assert_bits(&f, "acceleration", &run.a);
    n += assert_bits(&f, "residual", &run.r);
    // the defect is in the fixture: stale J at steps 4, 7, 8
    assert_eq!(run.threw, [0, 0, 0, 1, 0, 0, 1, 1, 0]);
    println!("solver_switch_shared_j (legacy): {n} values bit-identical");
}

/// One step of solver `id` alone (fresh model and manager) from `s`.
fn solve_alone(id: &str, s: &State) -> (Vec<f64>, Vec<f64>, Vec<f64>, usize, f64, bool) {
    let mut dm = DofManager::new("model_solver:dof_manager_default", Communicator::serial());
    let mut model = MyModel::new(s.force, gen_mesh(SWITCH_NODES), false, &mut dm).unwrap();
    dm.dofs_mut("disp")
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&s.u);
    dm.dofs_derivative_mut("disp", 1)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&s.v);
    dm.dofs_derivative_mut("disp", 2)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&s.a);
    dm.blocked_dofs_mut("disp")
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&s.blocked);
    let mut ms = ModelSolver::new(dm);
    add_switch_solver(&mut ms, &mut model, id);
    let threw = solve_counting(&mut ms, &mut model, id);
    let nls = ms.non_linear_solver(Some(id)).unwrap();
    let dm = &ms.dof_manager;
    (
        dm.dofs("disp").unwrap().as_slice().to_vec(),
        dm.dofs_derivative("disp", 1).unwrap().as_slice().to_vec(),
        dm.dofs_derivative("disp", 2).unwrap().as_slice().to_vec(),
        nls.nb_iterations(),
        nls.error(),
        threw,
    )
}

/// Default (UD-193 fix): every step converges, and every step equals —
/// bit for bit, iterations and error included — the same solver running
/// alone on a fresh DOF manager from the state before the step (the
/// Jacobian a solver uses no longer depends on which solver ran before).
/// The first three steps (before any switch back) equal the oracle's.
/// Static steps also satisfy the analytical bar solution. Fails before
/// the fix (steps 4, 7, 8 do not converge).
#[test]
fn sv03_solver_switch_shared_j_default_rebuilds_the_jacobian() {
    let f = case("solver_switch_shared_j");
    let tokens = switch_tokens();
    let run = run_switch(&tokens, UpstreamCompat::default());
    assert_eq!(run.threw, vec![0; run.solvers.len()], "{:?}", run.n_iter);
    assert!(run.converged.iter().all(|&c| c == 1));
    let n = SWITCH_NODES;
    let oracle_u = common::oracle::f64s(&f, "displacement");
    assert_eq!(&run.u[..3 * n], &oracle_u[..3 * n], "steps 1-3 as upstream");
    for (k, id) in run.solvers.iter().enumerate() {
        let (u, v, a, it, err, threw) = solve_alone(id, &run.before[k]);
        assert!(!threw, "step {}: {id} alone", k + 1);
        let row = k * n..(k + 1) * n;
        assert_eq!(
            it as i64,
            run.n_iter[k],
            "step {} ({id}): iterations",
            k + 1
        );
        assert_eq!(
            err.to_bits(),
            run.error[k].to_bits(),
            "step {} ({id}): error",
            k + 1
        );
        for (name, got, alone) in [
            ("u", &run.u[row.clone()], &u),
            ("v", &run.v[row.clone()], &v),
            ("a", &run.a[row.clone()], &a),
        ] {
            assert!(
                got.iter()
                    .zip(alone)
                    .all(|(x, y)| x.to_bits() == y.to_bits()),
                "step {} ({id}): {name} differs from the solver alone",
                k + 1
            );
        }
        if id == "static" {
            // E = A = 1, node 0 fixed: u = F x without the end node
            // blocked, else the linear interpolation to the blocked value.
            let s = &run.before[k];
            let x = |i: usize| i as f64 / (n - 1) as f64;
            let u_end = if s.blocked[n - 1] {
                s.u[n - 1]
            } else {
                s.force
            };
            let scale = u_end.abs().max(1e-300);
            for i in 0..n {
                let e = (run.u[k * n + i] - u_end * x(i)).abs() / scale;
                assert!(e < 1e-13, "step {}: static node {i}: rel {e:e}", k + 1);
            }
        }
    }
    println!(
        "solver_switch_shared_j (default): {} steps converge; iterations {:?}; each equal to its solver alone",
        run.solvers.len(),
        run.n_iter
    );
}

// ------------------------------------------------------- / UD-194

fn nl_static(compat: UpstreamCompat, criterion: SolveConvergenceCriteria) -> Result<(), Error> {
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.set_upstream_compat(compat);
    let mut model = MyModel::new(-0.1, gen_mesh(11), false, &mut dm).unwrap();
    let mut ms = ModelSolver::new(dm);
    add_switch_solver(&mut ms, &mut model, "static");
    ms.non_linear_solver_mut(None)
        .unwrap()
        .set_param("convergence_type", ParamValue::Enum(criterion.to_string()))
        .unwrap();
    model.trace.clear();
    let r = ms.solve_step(&mut model, None);
    if matches!(r, Err(Error::NotImplemented(_))) {
        // refused before any callback (agreed step of the solve)
        assert!(model.trace.is_empty(), "{:?}", model.trace);
    }
    r
}

/// `residual_mass_wgh` was never implemented upstream (UD-194): refused by
/// default before any callback; the compat switch runs upstream's branch.
/// Before the fix the default ran the defective branch.
#[test]
fn sv04_residual_mass_wgh_is_refused_by_default() {
    let r = nl_static(
        UpstreamCompat::default(),
        SolveConvergenceCriteria::ResidualMassWgh,
    );
    assert!(
        matches!(&r, Err(Error::NotImplemented(m)) if m.contains("UD-194")),
        "{r:?}"
    );
    let compat = UpstreamCompat {
        residual_mass_wgh_unrefreshed: true,
        ..UpstreamCompat::default()
    };
    // upstream: every non-converged iteration re-applies the first
    // increment (the residual is never refreshed), so even the linear bar
    // fails at max_iterations
    let r = nl_static(compat, SolveConvergenceCriteria::ResidualMassWgh);
    assert!(matches!(r, Err(Error::NotConverged { .. })), "{r:?}");
    assert!(
        nl_static(
            UpstreamCompat::default(),
            SolveConvergenceCriteria::Solution
        )
        .is_ok()
    );
}

// ------------------------------------------------------------------

/// UD-010 fix + `sv05_*`: a step that converges at the initial residual test
/// completes `after_solve_step(true)` and refreshes the increment to
/// `u − u_previous` (0 for pseudo-time); before the fix the increment kept
/// the previous step's value. The legacy hook mode keeps upstream's early
/// return (increment untouched).
#[test]
fn sv05_early_converged_step_refreshes_the_increment() {
    let run = |legacy: bool| {
        let mut dm = DofManager::new("dm", Communicator::serial());
        let mut model = MyModel::new(-1.0, gen_mesh(11), false, &mut dm).unwrap();
        dm.register_dofs_previous("disp").unwrap();
        dm.register_dofs_increment("disp").unwrap();
        let mut ms = ModelSolver::new(dm);
        add_switch_solver(&mut ms, &mut model, "static");
        {
            let nls = ms.non_linear_solver_mut(None).unwrap();
            nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
                .unwrap();
            nls.as_newton_raphson_mut()
                .unwrap()
                .set_legacy_hooks(legacy);
        }
        ms.solve_step(&mut model, None).unwrap();
        let first = ms
            .dof_manager
            .dofs_increment("disp")
            .unwrap()
            .as_slice()
            .to_vec();
        assert!(first.iter().any(|&x| x != 0.0), "first step moves the bar");
        model.trace.clear();
        ms.solve_step(&mut model, None).unwrap(); // already in equilibrium
        assert_eq!(ms.non_linear_solver(None).unwrap().nb_iterations(), 0);
        let second = ms
            .dof_manager
            .dofs_increment("disp")
            .unwrap()
            .as_slice()
            .to_vec();
        (first, second, model.trace.clone())
    };
    let (_, second, trace) = run(false);
    assert!(second.iter().all(|&x| x == 0.0), "{second:?}");
    assert_eq!(
        trace.last().map(String::as_str),
        Some("after_solve_step:true")
    );
    let (first, second, trace) = run(true);
    assert_eq!(first, second, "legacy: increment untouched (upstream)");
    assert!(!trace.iter().any(|t| t.starts_with("after_solve_step")));
}

// ------------------------------------------------------- / UD-195

/// Two generic DOFs, `M u̇ + K u = 0` with a consistent (coupled) `M` and a
/// diagonal `K`.
struct TwoDof {
    m: [[f64; 2]; 2],
    k: [f64; 2],
}

impl SolverCallback for TwoDof {
    fn matrix_type(&self, id: &str) -> MatrixType {
        match id {
            "K" | "M" => MatrixType::Symmetric,
            _ => MatrixType::NotDefined,
        }
    }
    fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> akantu_core::Result<()> {
        match id {
            "K" => {
                let k = dm.matrix_mut("K")?;
                k.zero();
                k.add(0, 0, self.k[0])?;
                k.add(1, 1, self.k[1])?;
            }
            "M" => {
                let m = dm.matrix_mut("M")?;
                m.zero();
                m.add(0, 0, self.m[0][0])?;
                m.add(0, 1, self.m[0][1])?;
                m.add(1, 1, self.m[1][1])?;
            }
            _ => {}
        }
        Ok(())
    }
    fn assemble_lumped_matrix(&mut self, _: &mut DofManager, _: &str) -> akantu_core::Result<()> {
        Ok(())
    }
    fn assemble_residual(&mut self, dm: &mut DofManager) -> akantu_core::Result<()> {
        if dm.has_matrix("K") {
            self.assemble_matrix(dm, "K")?;
            dm.assemble_mat_mul_dofs_to_residual("K", -1.0)
        } else {
            // forward Euler declares only M: the model computes −K u itself
            let u = dm.dofs("u")?.as_slice().to_vec();
            let r = Array::from_vec(vec![-self.k[0] * u[0], -self.k[1] * u[1]], 1)?;
            dm.assemble_to_residual("u", r.view(), 1.0)
        }
    }
}

fn solve2(m: [[f64; 2]; 2], b: [f64; 2]) -> [f64; 2] {
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    [
        (b[0] * m[1][1] - m[0][1] * b[1]) / det,
        (m[0][0] * b[1] - m[1][0] * b[0]) / det,
    ]
}

/// Forward Euler (temperature-rate solution) with a consistent mass for
/// `steps` steps of `dt`; returns `u` (`compat`: upstream's `K` read).
fn forward_euler(
    cb: &mut TwoDof,
    u0: [f64; 2],
    dt: f64,
    steps: usize,
    compat: UpstreamCompat,
) -> Result<[f64; 2], Error> {
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.set_upstream_compat(compat);
    dm.register_generic_dofs("u", 2, 1).unwrap();
    dm.register_dofs_derivative("u", 1).unwrap();
    dm.dofs_mut("u")
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u0);
    // consistent initial rate: M u̇ = −K u
    let v0 = solve2(cb.m, [-cb.k[0] * u0[0], -cb.k[1] * u0[1]]);
    dm.dofs_derivative_mut("u", 1)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&v0);
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Dynamic),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("fe", &opts, cb)?;
    ms.set_integration_scheme(
        "fe",
        "u",
        IntegrationSchemeType::ForwardEuler,
        SolutionType::TEMPERATURE_RATE,
        cb,
    )?;
    ms.set_time_step(dt, None)?;
    for _ in 0..steps {
        ms.solve_step(cb, None)?;
    }
    let u = ms.dof_manager.dofs("u")?;
    let u = u.as_slice();
    Ok([u[0], u[1]])
}

/// UD-195 fix: forward Euler with a consistent mass declares only `M` and
/// now solves with `J = M` (upstream and the earlier code: "matrix K does not
/// exist"). The history equals the exact explicit recursion
/// `u_{n+1} = u_n + Δt u̇_n`, `M u̇_{n+1} = −K u_{n+1}` (to round-off), and
/// the method is first-order accurate on the decoupled modes (error ratio
/// ≈ 2 when Δt halves). The compat switch reproduces upstream's error.
#[test]
fn sv06_forward_euler_consistent_mass_solves() {
    let m = [[2.0, 1.0], [1.0, 2.0]];
    let mut cb = TwoDof { m, k: [3.0, 1.0] };
    let u0 = [1.0, -0.5];
    let (dt, steps) = (0.01, 50);
    let got = forward_euler(&mut cb, u0, dt, steps, UpstreamCompat::default()).unwrap();
    let mut u = u0;
    let mut v = solve2(m, [-3.0 * u[0], -u[1]]);
    for _ in 0..steps {
        u = [u[0] + dt * v[0], u[1] + dt * v[1]];
        v = solve2(m, [-3.0 * u[0], -u[1]]);
    }
    for c in 0..2 {
        let e = (got[c] - u[c]).abs() / u[c].abs();
        assert!(
            e < 1e-12,
            "component {c}: {} vs {} (rel {e:e})",
            got[c],
            u[c]
        );
    }
    // first-order convergence on a decoupled system (M = I, K = λ I)
    let lambda = 2.0;
    let mut diag = TwoDof {
        m: [[1.0, 0.0], [0.0, 1.0]],
        k: [lambda, lambda],
    };
    let t = 0.5;
    let err = |n: usize, cb: &mut TwoDof| {
        let u = forward_euler(cb, [1.0, 1.0], t / n as f64, n, UpstreamCompat::default()).unwrap();
        (u[0] - (-lambda * t).exp()).abs()
    };
    let (e1, e2) = (err(50, &mut diag), err(100, &mut diag));
    let ratio = e1 / e2;
    assert!((1.9..2.1).contains(&ratio), "order ratio {ratio}");
    let compat = UpstreamCompat {
        generalized_trapezoidal_reads_k_when_e_is_zero: true,
        ..UpstreamCompat::default()
    };
    let r = forward_euler(&mut cb, u0, dt, 1, compat);
    assert!(
        matches!(&r, Err(e) if e.to_string().contains("K")),
        "upstream: K does not exist, got {r:?}"
    );
}

// ------------------------------------------------------------------

/// Family arrays shrunk through the public API (`family_mut`) are
/// reported as `DimensionMismatch` by the time-step solver and the
/// schemes; before the fix the increment case panicked at `time_step.rs:382`
/// and the derivative case silently truncated the update.
#[test]
fn sv07_resized_family_arrays_are_errors() {
    let fresh = || {
        let mut dm = DofManager::new("dm", Communicator::serial());
        let mut model = MyModel::new(-1.0, gen_mesh(11), false, &mut dm).unwrap();
        dm.register_dofs_previous("disp").unwrap();
        dm.register_dofs_increment("disp").unwrap();
        let mut ms = ModelSolver::new(dm);
        add_switch_solver(&mut ms, &mut model, "dynamic");
        (model, ms)
    };
    // increment shrunk
    let (mut model, mut ms) = fresh();
    ms.dof_manager
        .family_mut("disp")
        .unwrap()
        .increment
        .as_mut()
        .unwrap()
        .borrow_mut()
        .resize(3, 0.0);
    let r = ms.solve_step(&mut model, None);
    assert!(matches!(r, Err(Error::DimensionMismatch { .. })), "{r:?}");
    assert!(ms.dof_manager.poisoned().is_none());
    // velocity shrunk
    let (mut model, mut ms) = fresh();
    ms.dof_manager
        .dofs_derivative_mut("disp", 1)
        .unwrap()
        .resize(5, 0.0);
    let r = ms.solve_step(&mut model, None);
    assert!(matches!(r, Err(Error::DimensionMismatch { .. })), "{r:?}");
    // blocked flags shrunk
    let (mut model, mut ms) = fresh();
    ms.dof_manager
        .blocked_dofs_mut("disp")
        .unwrap()
        .resize(4, false);
    let r = ms.solve_step(&mut model, None);
    assert!(r.is_err(), "{r:?}");
}

// ------------------------------------------------------------------ UD-200

/// UD-200 (A part): upstream only asserts a defined common matrix type in
/// debug builds and creates `"J"` with `_mt_not_defined` in release; the
/// port reports an error when the model defines no type for the needed
/// matrices.
#[test]
fn ud200_undefined_common_matrix_type_is_an_error() {
    struct Untyped;
    impl SolverCallback for Untyped {
        fn matrix_type(&self, _: &str) -> MatrixType {
            MatrixType::NotDefined
        }
        fn assemble_residual(&mut self, _: &mut DofManager) -> akantu_core::Result<()> {
            Ok(())
        }
    }
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("u", 2, 1).unwrap();
    let mut ms = ModelSolver::new(dm);
    let mut cb = Untyped;
    ms.get_new_solver("s", &switch_options(TimeStepSolverType::Static), &mut cb)
        .unwrap();
    ms.set_integration_scheme(
        "s",
        "u",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &cb,
    )
    .unwrap();
    assert!(!ms.dof_manager.has_matrix("K"), "untyped K is not created");
    let r = ms.solve_step(&mut cb, None);
    assert!(
        matches!(&r, Err(e) if e.to_string().contains("No type defined for the matrices")),
        "{r:?}"
    );
}

/// A `previous` array resized through `family_mut`
/// is an error at the next predictor (`save_previous_dofs`), not a panic.
#[test]
fn sv2_02_resized_previous_dofs_is_an_error() {
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(-1.0, gen_mesh(11), false, &mut dm).unwrap();
    dm.register_dofs_previous("disp").unwrap();
    dm.register_dofs_increment("disp").unwrap();
    let mut ms = ModelSolver::new(dm);
    add_switch_solver(&mut ms, &mut model, "dynamic");
    ms.dof_manager
        .family_mut("disp")
        .unwrap()
        .previous
        .as_mut()
        .unwrap()
        .borrow_mut()
        .resize(4, 0.0);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ms.solve_step(&mut model, None)
    }));
    let r = r.expect("no panic");
    assert!(matches!(r, Err(Error::DimensionMismatch { .. })), "{r:?}");
    assert!(ms.dof_manager.save_previous_dofs("disp").is_err());
}
