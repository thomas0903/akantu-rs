// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (TS-004 / TS-005 / TS-006).
// Behaviour of upstream's `IntegrationScheme2ndOrder::assembleResidual`
// (`r -= C·u̇`, src/model/integrator/integration_scheme_2nd_order.cc),
// `NewmarkBeta::assembleJacobian` (`J = c·M + d·C + e·K`,
// newmark-beta.cc) and `CentralDifference` (needs `{M, C}`, `J = M + Δt/2·C`),
// exercised through a toy model that DECLARES the damping matrix `C`
// (upstream's MyModel declares `C` `_mt_not_defined`, so it never runs).
//! A damped bar with one free DOF (3 nodes, both ends blocked) and a
//! Rayleigh damping matrix `C = α M + β K` assembled by the model:
//!
//! * the Jacobian `J(free, free)` after a step equals `c·M + d·C + e·K` with
//!   the Newmark coefficients of the solution type (independent formulas);
//! * the trajectory of the full solver stack equals an independent scalar
//!   Newmark recurrence (floating-point agreement only), for the three
//!   solution types of the trapezoidal rule 2 and for the central
//!   difference;
//! * the trajectory of the average-acceleration Newmark scheme equals the
//!   analytical solution of the damped oscillator within the scheme's
//!   truncation error.

#[path = "common/my_model.rs"]
#[allow(dead_code)] // the toy model of the oracle tests; this file uses a subset
mod my_model;

use akantu_comm::Communicator;
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Array, Error, GhostType, Real, Result};
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback};
use my_model::{MyModel, SEG, gen_mesh};

/// Rayleigh coefficients of the probe-style toy: `C = ALPHA·M + BETA·K`.
const ALPHA: Real = 0.5;
const BETA: Real = 0.005;
const NB_NODES: usize = 3;
const DT: Real = 1.0e-3;
const U0: Real = 1.0;
const V0: Real = 0.3;
/// The only free DOF (node 1; nodes 0 and 2 are blocked).
const FREE: usize = 1;

/// `MyModel` (bar, `E = A = ρ = 1`) plus the damping matrix `C = αM + βK`
/// assembled element by element.
struct DampedBar {
    inner: MyModel,
}

impl DampedBar {
    /// Element matrices of the bar with `E = A = ρ = 1`:
    /// `k = (1/L)[[1,-1],[-1,1]]`, `m = (L/6)[[2,1],[1,2]]`, `c = αm + βk`.
    fn assemble_damping(&mut self, dm: &mut DofManager) -> Result<()> {
        dm.zero_matrix("C")?;
        let x = self.inner.mesh.nodes().as_slice().to_vec();
        let mut c_all_el = Array::new(4);
        for e in 0..NB_NODES - 1 {
            let l = (x[e + 1] - x[e]).abs();
            let (k, m) = (1.0 / l, l / 6.0);
            let (kd, ko) = (k, -k);
            let (md, mo) = (2.0 * m, m);
            c_all_el.push_tuple(&[
                ALPHA * md + BETA * kd,
                ALPHA * mo + BETA * ko,
                ALPHA * mo + BETA * ko,
                ALPHA * md + BETA * kd,
            ]);
        }
        dm.assemble_elemental_matrices_to_matrix(
            &self.inner.mesh,
            "C",
            "disp",
            c_all_el.view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
    }
}

impl SolverCallback for DampedBar {
    fn matrix_type(&self, _id: &str) -> MatrixType {
        MatrixType::Symmetric // including "C": the toy declares its damping
    }
    fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        if id == "C" {
            self.assemble_damping(dm)
        } else {
            self.inner.assemble_matrix(dm, id)
        }
    }
    fn assemble_lumped_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        self.inner.assemble_lumped_matrix(dm, id)
    }
    fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
        // no internal damping force here: `r -= C·u̇` belongs to the scheme
        self.inner.assemble_residual(dm)
    }
    fn assemble_residual_part(&mut self, dm: &mut DofManager, part: &str) -> Result<()> {
        self.inner.assemble_residual_part(dm, part)
    }
}

/// Scalar data of the one-DOF system, from the bar's closed forms.
struct Scalar {
    m: Real,
    k: Real,
    c: Real,
}

fn scalar_system() -> Scalar {
    let l = 1.0 / (NB_NODES - 1) as Real;
    // two elements around the free node
    let k = 2.0 * (1.0 / l);
    let m = 2.0 * (2.0 * l / 6.0);
    Scalar {
        m,
        k,
        c: ALPHA * m + BETA * k,
    }
}

/// Independent scalar Newmark recurrence (`β_N`, `γ_N` of the textbook, no
/// external force): `u* = u + Δt v + Δt²(½ − β) a`, `v* = v + Δt (1 − γ) a`,
/// `a₁ = −(c v* + k u*)/(m + γ Δt c + β Δt² k)`.
fn newmark_reference(s: &Scalar, beta: Real, gamma: Real, steps: usize) -> Vec<[Real; 3]> {
    let (mut u, mut v) = (U0, V0);
    let mut a = -(s.c * V0 + s.k * U0) / s.m;
    let mut out = vec![[u, v, a]];
    for _ in 0..steps {
        let us = u + DT * v + DT * DT * (0.5 - beta) * a;
        let vs = v + DT * (1.0 - gamma) * a;
        let a1 = -(s.c * vs + s.k * us) / (s.m + gamma * DT * s.c + beta * DT * DT * s.k);
        u = us + beta * DT * DT * a1;
        v = vs + gamma * DT * a1;
        a = a1;
        out.push([u, v, a]);
    }
    out
}

/// A driver stepping the damped bar; records `(u, v, a)` of the free DOF
/// after every step and keeps the solver for matrix inspection.
struct Run {
    history: Vec<[Real; 3]>,
    ms: ModelSolver,
}

fn run(scheme: IntegrationSchemeType, solution: SolutionType, steps: usize) -> Run {
    let s = scalar_system();
    let mut dm = DofManager::new("model_solver:dof_manager_default", Communicator::serial());
    let inner = MyModel::new(0.0, gen_mesh(NB_NODES), false, &mut dm).unwrap();
    let mut model = DampedBar { inner };
    model.inner.forces.iter_mut().for_each(|x| *x = 0.0);
    {
        let mut b = dm.blocked_dofs_mut("disp").unwrap();
        b.as_mut_slice()[0] = true;
        b.as_mut_slice()[NB_NODES - 1] = true;
    }
    // consistent initial state of the free node (blocked ones stay at rest)
    dm.dofs_mut("disp").unwrap().as_mut_slice()[FREE] = U0;
    dm.dofs_derivative_mut("disp", 1).unwrap().as_mut_slice()[FREE] = V0;
    dm.dofs_derivative_mut("disp", 2).unwrap().as_mut_slice()[FREE] = -(s.c * V0 + s.k * U0) / s.m;

    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Dynamic),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("dynamic", &opts, &mut model).unwrap();
    ms.set_integration_scheme("dynamic", "disp", scheme, solution, &model)
        .unwrap();
    ms.set_time_step(DT, Some("dynamic")).unwrap();

    let mut history = Vec::new();
    {
        let dm = &ms.dof_manager;
        history.push([
            dm.dofs("disp").unwrap().as_slice()[FREE],
            dm.dofs_derivative("disp", 1).unwrap().as_slice()[FREE],
            dm.dofs_derivative("disp", 2).unwrap().as_slice()[FREE],
        ]);
    }
    for _ in 0..steps {
        match ms.solve_step(&mut model, Some("dynamic")) {
            Ok(()) => {}
            Err(e @ Error::NotConverged { .. }) => panic!("not converged: {e}"),
            Err(e) => panic!("{e}"),
        }
        let dm = &ms.dof_manager;
        history.push([
            dm.dofs("disp").unwrap().as_slice()[FREE],
            dm.dofs_derivative("disp", 1).unwrap().as_slice()[FREE],
            dm.dofs_derivative("disp", 2).unwrap().as_slice()[FREE],
        ]);
    }
    Run { history, ms }
}

/// `(c, d, e)` of the Newmark scheme with textbook `β_N`, `γ_N` for the
/// unknown `solution`: `δ(a, v, u) = (c, d, e) δx`, from `δv = γΔt δa`,
/// `δu = βΔt² δa`.
fn coefficients(solution: SolutionType, beta: Real, gamma: Real) -> (Real, Real, Real) {
    match solution {
        SolutionType::Acceleration => (1.0, gamma * DT, beta * DT * DT),
        SolutionType::Velocity => (1.0 / (gamma * DT), 1.0, beta * DT / gamma),
        SolutionType::Displacement => (1.0 / (beta * DT * DT), gamma / (beta * DT), 1.0),
        SolutionType::NotDefined => unreachable!(),
    }
}

fn rel(a: Real, b: Real) -> Real {
    (a - b).abs() / b.abs().max(1.0)
}

/// `J(free, free)` of the last step equals `c·M + d·C + e·K` (the free
/// diagonal entry of every matrix is read back from the DOF manager).
fn check_jacobian(r: &Run, solution: SolutionType, beta: Real, gamma: Real, with_k: bool) {
    let dm = &r.ms.dof_manager;
    let s = scalar_system();
    let (c, d, e) = coefficients(solution, beta, gamma);
    let j = dm.matrix("J").unwrap().get(FREE as _, FREE as _);
    let m_entry = dm.matrix("M").unwrap().get(FREE as _, FREE as _);
    let c_entry = dm.matrix("C").unwrap().get(FREE as _, FREE as _);
    // the model's own matrices carry the closed-form values
    assert!(rel(m_entry, s.m) < 1e-14, "M: {m_entry} vs {}", s.m);
    assert!(rel(c_entry, s.c) < 1e-14, "C: {c_entry} vs {}", s.c);
    let k_term = if with_k { e * s.k } else { 0.0 };
    let expected = c * s.m + d * s.c + k_term;
    assert!(
        rel(j, expected) < 1e-13,
        "J(free,free) = {j}, expected c·M + d·C (+ e·K) = {expected} (c, d, e = {c}, {d}, {e})"
    );
    // ... and the damping part is a substantial share of it (so that a
    // missing `d·C` cannot hide in the tolerance)
    assert!((d * s.c).abs() / expected.abs() > 1e-4);
}

/// Trapezoidal rule 2 (= Newmark `β_N = 1/4`, `γ_N = 1/2`, average
/// acceleration) with each of its three solution types.
#[test]
fn newmark_average_acceleration_with_damping_matrix_all_solution_types() {
    let steps = 1000;
    let s = scalar_system();
    let reference = newmark_reference(&s, 0.25, 0.5, steps);
    for solution in [
        SolutionType::Acceleration,
        SolutionType::Velocity,
        SolutionType::Displacement,
    ] {
        let r = run(IntegrationSchemeType::TrapezoidalRule2, solution, steps);
        // trajectory: the full stack against the scalar recurrence
        let mut worst = 0.0_f64;
        for (got, want) in r.history.iter().zip(&reference) {
            for q in 0..3 {
                worst = worst.max(rel(got[q], want[q]));
            }
        }
        // Both sides evaluate the same linear recurrence in different
        // orders: only floating-point round-off (condition number of the
        // displacement form ≈ 1/Δt² = 1e6 → a few 1e-10 at most) separates
        // them, while omitting `r -= C·u̇` or `d·C` changes the trajectory
        // at order 1e-1 (c/m = 0.56).
        assert!(
            worst < 1e-8,
            "{solution:?}: worst relative difference {worst:e}"
        );
        check_jacobian(&r, solution, 0.25, 0.5, true);
    }
}

/// The average-acceleration trajectory against the analytical solution of
/// the damped oscillator `m ü + c u̇ + k u = 0`.
#[test]
fn newmark_average_acceleration_matches_the_damped_oscillator() {
    let steps = 1000;
    let s = scalar_system();
    let r = run(
        IntegrationSchemeType::TrapezoidalRule2,
        SolutionType::Acceleration,
        steps,
    );
    // underdamped: u = e^{-ζωt} (u0 cos ω_d t + (v0 + ζω u0)/ω_d sin ω_d t)
    let omega2 = s.k / s.m;
    let zeta_omega = s.c / (2.0 * s.m);
    assert!(omega2 > zeta_omega * zeta_omega, "underdamped");
    let omega_d = (omega2 - zeta_omega * zeta_omega).sqrt();
    let exact = |t: Real| {
        (-zeta_omega * t).exp()
            * (U0 * (omega_d * t).cos() + (V0 + zeta_omega * U0) / omega_d * (omega_d * t).sin())
    };
    let mut worst = 0.0_f64;
    for (n, h) in r.history.iter().enumerate() {
        worst = worst.max((h[0] - exact(n as Real * DT)).abs());
    }
    // Truncation error of the average-acceleration rule: relative phase
    // error ≈ (ω Δt)²/24 per radian (ω ≈ 3.46, Δt = 1e-3 → 5e-7), over
    // ω t ≈ 3.5 rad and amplitude ≤ 1: ≲ 2e-6. The tolerance 1e-5 is five
    // times that bound; the undamped solution would differ by ≈ 0.3.
    assert!(worst < 1e-5, "max |u − u_exact| = {worst:e}");
}

/// Central difference (Newmark `β_N = 0`, `γ_N = 1/2`; needs `{M, C}` only)
/// with a damping matrix: `J = M + (Δt/2)·C`, no `K` in `J`.
#[test]
fn central_difference_with_damping_matrix() {
    let steps = 1000;
    let s = scalar_system();
    // stability: Δt < 2/ω_max·(1 + damping): ω² = k/m = 12, Δt = 1e-3
    let reference = newmark_reference(&s, 0.0, 0.5, steps);
    let r = run(
        IntegrationSchemeType::CentralDifference,
        SolutionType::Acceleration,
        steps,
    );
    let mut worst = 0.0_f64;
    for (got, want) in r.history.iter().zip(&reference) {
        for q in 0..3 {
            worst = worst.max(rel(got[q], want[q]));
        }
    }
    // same argument as for the Newmark test (here no 1/Δt² conditioning)
    assert!(worst < 1e-10, "worst relative difference {worst:e}");
    check_jacobian(&r, SolutionType::Acceleration, 0.0, 0.5, false);
    // `K` is not a needed matrix of the central difference
    assert!(!r.ms.dof_manager.has_matrix("K"));
}
