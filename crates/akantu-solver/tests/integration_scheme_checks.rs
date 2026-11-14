// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs independent checks (no upstream counterpart) of the solver stack
// ported from Akantu (EPFL/LSMS) src/model/common/{integration_scheme/,
// non_linear_solver/, time_step_solvers/}.

//! Independent mathematical checks of the time-step solvers and
//! integration schemes (not oracle comparisons):
//!
//! * order of accuracy of the Newmark-β presets on `ü + ω²u = 0`;
//! * exact energy conservation of the trapezoidal rule (average
//!   acceleration) on a free spring chain;
//! * stability limits: central difference (lumped, `ωΔt < 2`), trapezoidal
//!   rule unconditionally stable, forward Euler (lumped, `λΔt < 2`),
//!   backward Euler unconditionally stable;
//! * static Newton–Raphson equals the direct solution (and the analytical
//!   bar solution); a linear problem converges in one Newton iteration with
//!   the residual criterion.

mod common;

use akantu_comm::Communicator;
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Array, Real, Result};
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback};
use akantu_validation::reduce::NanMinMax;

use common::my_model::{MyModel, gen_mesh};

/// A chain of `n` unit masses linked by springs `k` (free ends), generic
/// DOFs: `M u̇̇ + K u = 0` with consistent or lumped `M = I`.
struct Chain {
    n: usize,
    k: Real,
}

impl Chain {
    fn stiffness(&self, i: usize, j: usize) -> Real {
        let n = self.n;
        if self.n == 1 {
            return self.k; // single oscillator: K = k
        }
        if i == j {
            if i == 0 || i == n - 1 {
                self.k
            } else {
                2.0 * self.k
            }
        } else if i.abs_diff(j) == 1 {
            -self.k
        } else {
            0.0
        }
    }
}

impl SolverCallback for Chain {
    fn matrix_type(&self, id: &str) -> MatrixType {
        if id == "C" {
            MatrixType::NotDefined
        } else {
            MatrixType::Symmetric
        }
    }
    fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        let n = self.n as i64;
        match id {
            "K" => {
                let m = dm.matrix_mut("K")?;
                m.zero();
                for i in 0..n {
                    for j in i..n.min(i + 2) {
                        let v = self.stiffness(i as usize, j as usize);
                        if v != 0.0 {
                            m.add(i, j, v)?;
                        }
                    }
                }
            }
            "M" => {
                let m = dm.matrix_mut("M")?;
                m.zero();
                for i in 0..n {
                    m.add(i, i, 1.0)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn assemble_lumped_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        let ones = Array::from_vec(vec![1.0; self.n], 1)?;
        dm.zero_lumped_matrix(id)?;
        dm.assemble_to_lumped_matrix("u", ones.view(), id, 1.0)
    }
    fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
        let u = dm.dofs("u")?.as_slice().to_vec();
        let f: Vec<Real> = (0..self.n)
            .map(|i| {
                -(0..self.n)
                    .map(|j| self.stiffness(i, j) * u[j])
                    .sum::<Real>()
            })
            .collect();
        let fa = Array::from_vec(f, 1)?;
        dm.assemble_to_residual("u", fa.view(), 1.0)
    }
}

fn solver_for(
    chain: &mut Chain,
    tss: TimeStepSolverType,
    nls: NonLinearSolverType,
    scheme: IntegrationSchemeType,
    st: SolutionType,
    dt: Real,
    order: usize,
) -> ModelSolver {
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("u", chain.n, 1).unwrap();
    for o in 1..=order {
        dm.register_dofs_derivative("u", o).unwrap();
    }
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(tss),
        non_linear_solver_type: Some(nls),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &opts, chain).unwrap();
    ms.set_integration_scheme("s", "u", scheme, st, chain)
        .unwrap();
    ms.set_time_step(dt, None).unwrap();
    ms
}

/// `ü + u = 0`, `u(0) = 1`, `u̇(0) = 0`, consistent `ü(0) = −1`, integrated
/// to `T = 1`: `|u(T) − cos 1|`.
fn oscillator_error(scheme: IntegrationSchemeType, st: SolutionType, dt: Real) -> Real {
    let mut chain = Chain { n: 1, k: 1.0 };
    let mut ms = solver_for(
        &mut chain,
        TimeStepSolverType::Dynamic,
        NonLinearSolverType::NewtonRaphson,
        scheme,
        st,
        dt,
        2,
    );
    ms.dof_manager.dofs_mut("u").unwrap().as_mut_slice()[0] = 1.0;
    ms.dof_manager
        .dofs_derivative_mut("u", 2)
        .unwrap()
        .as_mut_slice()[0] = -1.0;
    let steps = (1.0 / dt).round() as usize;
    for _ in 0..steps {
        ms.solve_step(&mut chain, None).unwrap();
    }
    (ms.dof_manager.dofs("u").unwrap().as_slice()[0] - 1.0f64.cos()).abs()
}

#[test]
fn newmark_presets_order_of_accuracy() {
    use IntegrationSchemeType as T;
    for (scheme, st, min_order) in [
        (T::TrapezoidalRule2, SolutionType::Displacement, 1.95),
        (T::TrapezoidalRule2, SolutionType::Acceleration, 1.95),
        (T::LinearAcceleration, SolutionType::Velocity, 1.95),
        (T::CentralDifference, SolutionType::Acceleration, 1.95),
        (T::FoxGoodwin, SolutionType::Displacement, 3.9),
    ] {
        let errs: Vec<Real> = [0.05, 0.025, 0.0125]
            .iter()
            .map(|&dt| oscillator_error(scheme, st, dt))
            .collect();
        let rates: Vec<Real> = errs.windows(2).map(|w| (w[0] / w[1]).log2()).collect();
        println!("{scheme:?}/{st:?}: errors {errs:?}, observed orders {rates:.3?}");
        for r in &rates {
            assert!(
                *r >= min_order,
                "{scheme:?}: observed order {r} < {min_order}"
            );
        }
    }
}

fn chain_energy(ms: &ModelSolver, chain: &Chain) -> Real {
    let u = ms.dof_manager.dofs("u").unwrap();
    let u = u.as_slice();
    let v = ms.dof_manager.dofs_derivative("u", 1).unwrap();
    let v = v.as_slice();
    let kin: Real = v.iter().map(|x| 0.5 * x * x).sum();
    let pot: Real = (0..chain.n)
        .map(|i| {
            (0..chain.n)
                .map(|j| 0.5 * u[i] * chain.stiffness(i, j) * u[j])
                .sum::<Real>()
        })
        .sum();
    kin + pot
}

#[test]
fn trapezoidal_rule_conserves_energy() {
    let mut chain = Chain { n: 10, k: 50.0 };
    for dt in [0.01, 1.0] {
        let mut ms = solver_for(
            &mut chain,
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
            IntegrationSchemeType::TrapezoidalRule2,
            SolutionType::Displacement,
            dt,
            2,
        );
        {
            let dm = &mut ms.dof_manager;
            let mut u = dm.dofs_mut("u").unwrap();
            let u = u.as_mut_slice();
            u[0] = 0.1;
            u[9] = -0.05;
        }
        // consistent initial acceleration a = −K u (M = I)
        let u0 = ms.dof_manager.dofs("u").unwrap().as_slice().to_vec();
        let a0: Vec<Real> = (0..10)
            .map(|i| -(0..10).map(|j| chain.stiffness(i, j) * u0[j]).sum::<Real>())
            .collect();
        ms.dof_manager
            .dofs_derivative_mut("u", 2)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&a0);
        let e0 = chain_energy(&ms, &chain);
        let mut worst = 0.0f64;
        for _ in 0..500 {
            ms.solve_step(&mut chain, None).unwrap();
            worst = worst.nan_max(((chain_energy(&ms, &chain) - e0) / e0).abs());
        }
        println!("trapezoidal rule, dt = {dt}: max relative energy drift {worst:e}");
        assert!(worst < 1e-10, "energy drift {worst:e} at dt = {dt}");
    }
}

/// Amplitude after `steps` steps of `scheme` on `ü + ω²u = 0` (ω = 1).
fn oscillator_amplitude(
    tss: TimeStepSolverType,
    nls: NonLinearSolverType,
    scheme: IntegrationSchemeType,
    st: SolutionType,
    dt: Real,
    steps: usize,
) -> Real {
    let mut chain = Chain { n: 1, k: 1.0 };
    let mut ms = solver_for(&mut chain, tss, nls, scheme, st, dt, 2);
    ms.dof_manager.dofs_mut("u").unwrap().as_mut_slice()[0] = 1.0;
    ms.dof_manager
        .dofs_derivative_mut("u", 2)
        .unwrap()
        .as_mut_slice()[0] = -1.0;
    let mut amp: Real = 0.0;
    for _ in 0..steps {
        ms.solve_step(&mut chain, None).unwrap();
        amp = amp.nan_max(ms.dof_manager.dofs("u").unwrap().as_slice()[0].abs());
    }
    amp
}

#[test]
fn central_difference_is_conditionally_stable() {
    let run = |dt| {
        oscillator_amplitude(
            TimeStepSolverType::DynamicLumped,
            NonLinearSolverType::Lumped,
            IntegrationSchemeType::CentralDifference,
            SolutionType::Acceleration,
            dt,
            400,
        )
    };
    let stable = run(1.9);
    let unstable = run(2.1);
    println!("central difference: amplitude {stable:e} at ωΔt = 1.9, {unstable:e} at 2.1");
    assert!(stable < 10.0, "ωΔt = 1.9 must stay bounded: {stable}");
    assert!(unstable > 1e6, "ωΔt = 2.1 must diverge: {unstable}");
}

#[test]
fn trapezoidal_rule_is_unconditionally_stable() {
    for dt in [2.1, 10.0, 100.0] {
        let amp = oscillator_amplitude(
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
            IntegrationSchemeType::TrapezoidalRule2,
            SolutionType::Displacement,
            dt,
            400,
        );
        assert!(amp <= 1.0 + 1e-12, "dt = {dt}: amplitude {amp}");
    }
}

/// `u̇ + λu = 0`, `u(0) = 1`: `max |u|` after `steps` steps.
fn first_order_amplitude(
    lumped: bool,
    scheme: IntegrationSchemeType,
    st: SolutionType,
    lambda_dt: Real,
    steps: usize,
) -> Real {
    let mut chain = Chain { n: 1, k: 1.0 };
    let (tss, nls) = if lumped {
        (
            TimeStepSolverType::DynamicLumped,
            NonLinearSolverType::Lumped,
        )
    } else {
        (
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
        )
    };
    let mut ms = solver_for(&mut chain, tss, nls, scheme, st, lambda_dt, 1);
    ms.dof_manager.dofs_mut("u").unwrap().as_mut_slice()[0] = 1.0;
    // consistent initial rate u̇ = −λu
    ms.dof_manager
        .dofs_derivative_mut("u", 1)
        .unwrap()
        .as_mut_slice()[0] = -1.0;
    let mut amp: Real = 0.0;
    for _ in 0..steps {
        ms.solve_step(&mut chain, None).unwrap();
        amp = amp.nan_max(ms.dof_manager.dofs("u").unwrap().as_slice()[0].abs());
    }
    amp
}

#[test]
fn first_order_stability_limits() {
    use IntegrationSchemeType as T;
    let fe = |ldt| {
        first_order_amplitude(
            true,
            T::ForwardEuler,
            SolutionType::TEMPERATURE_RATE,
            ldt,
            200,
        )
    };
    let (s, u) = (fe(1.9), fe(2.1));
    println!("forward Euler (lumped): {s:e} at λΔt = 1.9, {u:e} at 2.1");
    assert!(s <= 1.0 + 1e-12);
    assert!(u > 1e6);
    for ldt in [0.5, 5.0, 500.0] {
        let be =
            first_order_amplitude(false, T::BackwardEuler, SolutionType::TEMPERATURE, ldt, 200);
        let tr1 = first_order_amplitude(
            false,
            T::TrapezoidalRule1,
            SolutionType::TEMPERATURE,
            ldt,
            200,
        );
        assert!(be <= 1.0 + 1e-12, "backward Euler at λΔt = {ldt}: {be}");
        assert!(
            tr1 <= 1.0 + 1e-12,
            "trapezoidal rule 1 at λΔt = {ldt}: {tr1}"
        );
    }
}

/// Static Newton–Raphson on the upstream bar equals the direct solution of
/// `K u = F` (Thomas algorithm, independent) and the analytical
/// `u(x) = F x / (EA)`; with the residual criterion a linear problem
/// converges in exactly one iteration.
#[test]
fn static_solve_equals_direct_solve_and_converges_in_one_iteration() {
    let n = 100;
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(-10.0, gen_mesh(n), false, &mut dm).unwrap();
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
    ms.non_linear_solver_mut(None)
        .unwrap()
        .set_param("convergence_type", ParamValue::Enum("residual".into()))
        .unwrap();
    ms.solve_step(&mut model, None).unwrap();
    assert_eq!(ms.non_linear_solver(None).unwrap().nb_iterations(), 1);
    let u = ms.dof_manager.dofs("disp").unwrap().as_slice().to_vec();
    // Thomas algorithm on the free DOFs 1..n−1: k = (n − 1) per element.
    let k = (n - 1) as Real;
    let m = n - 1;
    let (mut a, mut b, mut c, mut d) = (vec![-k; m], vec![2.0 * k; m], vec![-k; m], vec![0.0; m]);
    b[m - 1] = k;
    d[m - 1] = -10.0;
    a[0] = 0.0;
    c[m - 1] = 0.0;
    for i in 1..m {
        let w = a[i] / b[i - 1];
        b[i] -= w * c[i - 1];
        d[i] -= w * d[i - 1];
    }
    let mut x = vec![0.0; m];
    x[m - 1] = d[m - 1] / b[m - 1];
    for i in (0..m - 1).rev() {
        x[i] = (d[i] - c[i] * x[i + 1]) / b[i];
    }
    let mut worst_direct = 0.0f64;
    let mut worst_exact = 0.0f64;
    for i in 1..n {
        worst_direct = worst_direct.nan_max((u[i] - x[i - 1]).abs() / 10.0);
        let xi = i as Real / (n - 1) as Real;
        worst_exact = worst_exact.nan_max((u[i] - (-10.0 * xi)).abs() / 10.0);
    }
    println!("static NR vs direct: {worst_direct:e}; vs analytical: {worst_exact:e}");
    assert!(worst_direct < 1e-12);
    assert!(worst_exact < 1e-12);
    assert_eq!(u[0], 0.0);
}
