// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs translation of Akantu (EPFL/LSMS) test/python_fe/dynamic_solver.py
// (DynamicSolver), test/python_fe/truss_fe.py (TrussFE) and test/python_fe/fe.py
// (Solver._zero_rows), used as an independent check of
// test/test_model/test_common/test_model_solver/test_model_solver_dynamic.cc
// (EXPLICIT=false, TEST-074) through the oracle case solver_model/dynamic_implicit.

//! TEST-232 / GOLD-010: upstream's NumPy truss model
//! and Newmark solver `python_fe.DynamicSolver` (`alpha = beta = 1/2`,
//! displacement increments, Newton loop until `‖δ‖ < 1e-10`), translated,
//! run on TEST-074's implicit truss: 201 nodes on `[0, 1]`, `E = A = ρ = 1`,
//! consistent mass, no load (the test zeroes the forces), the sinusoidal
//! pulse as initial displacement and velocity of every node, both ends
//! blocked with the pulse's displacement and velocity imposed before each of
//! the 400 steps of `Δt = 1e-3`. The initial state and the boundary values
//! are read from the oracle case (`inputs/*`, upstream's glibc `sin`/`cos`),
//! and the displacements are compared with its snapshots (every 40 steps),
//! which the port reproduces bit for bit
//! (`test_model_solver::test_model_solver_dynamic_implicit`).
//!
//! **The 1–5 % difference.** Upstream only uses this solver in a matplotlib
//! animation (`test_model_solver_dynamic.py`, which compares nothing, on
//! another problem: a tip load, no pulse). Run literally on TEST-074 it
//! leaves the oracle at the percent level. The cause is the script's own
//! treatment of the Dirichlet DOFs, not a parameter and not an Akantu
//! defect: `_assembleResidual` does not zero the rows of the blocked DOFs
//! and `_zero_rows` keeps their diagonal, so every Newton iteration solves
//! `J_bb δ_b = r_b ≠ 0` (the reaction) at the blocked DOFs and the corrector
//! adds `δ_b / (αβΔt²)` to their **acceleration** (the displacement is
//! re-imposed by the `u` property, the velocity is not); the predictor also
//! moves the blocked velocity and acceleration. Through the consistent mass
//! (`M_{1,0} = ρAL_e/6`) the non-zero boundary acceleration enters the
//! inertia of the neighbouring nodes. Akantu's Newmark scheme skips the
//! blocked DOFs in the predictor and corrector and solves with zero
//! increments there (their acceleration stays 0). With exactly that
//! Dirichlet treatment and nothing else changed, the translated solver
//! agrees with the oracle to the solver's tolerance. The test shows both:
//! the literal script differs by more than 1e-3 (relative), the script with
//! Akantu's Dirichlet treatment agrees to `1e-10` relative to the largest
//! displacement (upstream's tolerance of `python_fe`; measured values in
//! `docs/NUMERICAL_VALIDATION.md`).

mod common;

use akantu_core::Real;

use common::oracle::{case, f64s, param};

/// `TrussFE` / `DynamicSolver` state (tridiagonal matrices: the truss
/// matrices and the Jacobian with zeroed rows are tridiagonal).
struct Truss {
    n: usize,
    /// Lower, diagonal, upper bands of `M` and `K`.
    m: [Vec<Real>; 3],
    k: [Vec<Real>; 3],
    u: Vec<Real>,
    v: Vec<Real>,
    a: Vec<Real>,
    blocked: Vec<(usize, Real)>,
}

/// How the blocked DOFs are treated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dirichlet {
    /// `python_fe` as written: predictor and corrector on every DOF, the
    /// displacement re-imposed, the residual rows of the blocked DOFs kept.
    Script,
    /// Akantu's Newmark scheme: the blocked DOFs are skipped by the
    /// predictor and the corrector, their increments are zero.
    Akantu,
}

impl Truss {
    /// `TrussFE(Ne = ne)` with `A = E = L = ρ = 1`: `Me = ρ A Le / 6 [[2, 1],
    /// [1, 2]]`, `Ke = E A / Le [[1, −1], [−1, 1]]`.
    fn new(ne: usize) -> Self {
        let n = ne + 1;
        let le = 1.0 / ne as Real;
        let mut m = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
        let mut k = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
        let (me, ke) = (le / 6.0, 1.0 / le);
        for i in 0..ne {
            m[1][i] += 2.0 * me;
            m[1][i + 1] += 2.0 * me;
            m[2][i] += me;
            m[0][i + 1] += me;
            k[1][i] += ke;
            k[1][i + 1] += ke;
            k[2][i] -= ke;
            k[0][i + 1] -= ke;
        }
        Self {
            n,
            m,
            k,
            u: vec![0.0; n],
            v: vec![0.0; n],
            a: vec![0.0; n],
            blocked: Vec::new(),
        }
    }

    fn product(b: &[Vec<Real>; 3], x: &[Real], i: usize) -> Real {
        let mut s = b[1][i] * x[i];
        if i > 0 {
            s += b[0][i] * x[i - 1];
        }
        if i + 1 < x.len() {
            s += b[2][i] * x[i + 1];
        }
        s
    }

    /// `applyDirichletBC`: `u[b] = value`.
    fn apply_dirichlet(&mut self) {
        for &(b, v) in &self.blocked {
            self.u[b] = v;
        }
    }

    fn is_blocked(&self, i: usize) -> bool {
        self.blocked.iter().any(|&(b, _)| b == i)
    }

    /// `DynamicSolver.solveStep` (`delta_t`, `alpha = beta = 1/2`,
    /// `type = "disp"`, `tolerance = 1e-10`, `max_iterations = 100`);
    /// returns the number of Newton loops.
    fn solve_step(&mut self, dt: Real, mode: Dirichlet) -> usize {
        let (alpha, beta) = (0.5, 0.5);
        let (ce, cv, ca) = (1.0, 1.0 / (alpha * dt), 1.0 / (alpha * beta * dt * dt));
        // _predictor: u += dt v + dt²/2 a (u getter/setter re-impose the
        // Dirichlet values), v += dt a
        self.apply_dirichlet();
        for i in 0..self.n {
            if mode == Dirichlet::Akantu && self.is_blocked(i) {
                continue;
            }
            self.u[i] += dt * self.v[i] + dt * dt / 2.0 * self.a[i];
            self.v[i] += dt * self.a[i];
        }
        self.apply_dirichlet();
        let mut nloops = 0;
        loop {
            // _assembleJacobian: J = e K + c M; applyDirichletBC; zero rows
            let band = |b: usize| -> Vec<Real> {
                self.k[b]
                    .iter()
                    .zip(&self.m[b])
                    .map(|(k, m)| ce * k + ca * m)
                    .collect()
            };
            let mut j = [band(0), band(1), band(2)];
            self.apply_dirichlet();
            for &(b, _) in &self.blocked {
                j[0][b] = 0.0;
                j[2][b] = 0.0;
            }
            // _assembleResidual: r = f_ext − f_int − M a (f_ext = 0)
            let mut r: Vec<Real> = (0..self.n)
                .map(|i| -Self::product(&self.k, &self.u, i) - Self::product(&self.m, &self.a, i))
                .collect();
            if mode == Dirichlet::Akantu {
                for &(b, _) in &self.blocked {
                    r[b] = 0.0;
                }
            }
            let delta = thomas(j, r);
            // _corrector
            for (i, d) in delta.iter().enumerate() {
                if mode == Dirichlet::Akantu && self.is_blocked(i) {
                    continue;
                }
                self.u[i] += ce * d;
                self.v[i] += cv * d;
                self.a[i] += ca * d;
            }
            self.apply_dirichlet();
            nloops += 1;
            let error = delta.iter().map(|d| d * d).sum::<Real>().sqrt();
            if error < 1e-10 || nloops > 100 {
                break;
            }
        }
        assert!(nloops < 100, "The solver did not converge");
        nloops
    }
}

/// Tridiagonal solve (in place of SciPy's `spsolve`; the Jacobian is
/// diagonally dominant).
fn thomas(mut j: [Vec<Real>; 3], mut r: Vec<Real>) -> Vec<Real> {
    let n = r.len();
    for i in 1..n {
        let w = j[0][i] / j[1][i - 1];
        j[1][i] -= w * j[2][i - 1];
        r[i] -= w * r[i - 1];
    }
    let mut x = vec![0.0; n];
    x[n - 1] = r[n - 1] / j[1][n - 1];
    for i in (0..n - 1).rev() {
        x[i] = (r[i] - j[2][i] * x[i + 1]) / j[1][i];
    }
    x
}

/// Runs TEST-074's implicit truss with the translated solver; returns the
/// largest relative difference of the displacement snapshots from the
/// oracle's (relative to the largest displacement of that snapshot), the
/// worst snapshot step, and the largest boundary acceleration.
fn run(mode: Dirichlet) -> (Real, usize, Real) {
    let f = case("dynamic_implicit");
    let n = param(&f, "nb_nodes").as_u64().unwrap() as usize;
    let steps = param(&f, "max_steps").as_u64().unwrap() as usize;
    let dt = param(&f, "time_step").as_f64().unwrap();
    let snap_steps: Vec<usize> = param(&f, "snapshot_steps")
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_u64().unwrap() as usize)
        .collect();
    let u0 = f64s(&f, "inputs/initial_displacement");
    let v0 = f64s(&f, "inputs/initial_velocity");
    let border = f64s(&f, "inputs/border_bc");
    let snaps = f64s(&f, "snapshots/displacement");
    let mut t = Truss::new(n - 1);
    t.u.copy_from_slice(&u0);
    t.v.copy_from_slice(&v0);
    let mut worst = (0.0, 0);
    let mut a_bnd: Real = 0.0;
    for i in 1..=steps {
        // applyBC(Sinusoidal(t = dt (i − 1)), "border"): the displacement
        // (blocked values) and the velocity of both ends
        let b = &border[(i - 1) * 4..i * 4];
        t.blocked = vec![(0, b[0]), (n - 1, b[2])];
        t.v[0] = b[1];
        t.v[n - 1] = b[3];
        t.solve_step(dt, mode);
        a_bnd = a_bnd.max(t.a[0].abs()).max(t.a[n - 1].abs());
        if let Some(s) = snap_steps.iter().position(|&s| s == i) {
            let want = &snaps[s * n..(s + 1) * n];
            let scale = want.iter().fold(0.0, |m: Real, x| m.max(x.abs()));
            let d =
                t.u.iter()
                    .zip(want)
                    .fold(0.0, |m: Real, (x, y)| m.max((x - y).abs()))
                    / scale;
            if d > worst.0 {
                worst = (d, i);
            }
        }
    }
    (worst.0, worst.1, a_bnd)
}

#[test]
fn test_model_solver_dynamic_implicit_python_fe_check() {
    let (script, script_step, script_abnd) = run(Dirichlet::Script);
    let (akantu, akantu_step, akantu_abnd) = run(Dirichlet::Akantu);
    println!(
        "python_fe DynamicSolver on TEST-074 (implicit): as written {script:.3e} (step \
         {script_step}, max |a| at the blocked DOFs {script_abnd:.3e}); with Akantu's Dirichlet \
         treatment {akantu:.3e} (step {akantu_step}, max |a| at the blocked DOFs {akantu_abnd})"
    );
    println!("as written: {script:.16e}, max |a| at the blocked DOFs {script_abnd:.16e}");
    // the script's own Dirichlet treatment: non-zero boundary acceleration,
    // percent-level departure (the difference seen earlier), pinned at the
    // values of upstream's unchanged `test/python_fe` package run on the
    // same inputs (NumPy 2.2.4, SciPy 1.15.3
    // `spsolve`, printed with 7 significant digits: 5.336292e-2 at step
    // 280, boundary acceleration 1.276660e2; rule PFE-1 of
    // NUMERICAL_VALIDATION)
    const UPSTREAM_SCRIPT: Real = 5.336292e-2;
    const UPSTREAM_SCRIPT_ABND: Real = 1.276660e2;
    const PFE1_REL: Real = 1e-6;
    assert_eq!(script_step, 280, "worst snapshot of the script as written");
    assert!(
        (script - UPSTREAM_SCRIPT).abs() <= PFE1_REL * UPSTREAM_SCRIPT,
        "{script:e} vs upstream's script {UPSTREAM_SCRIPT:e}"
    );
    assert!(
        (script_abnd - UPSTREAM_SCRIPT_ABND).abs() <= PFE1_REL * UPSTREAM_SCRIPT_ABND,
        "{script_abnd:e} vs upstream's script {UPSTREAM_SCRIPT_ABND:e}"
    );
    // Akantu's treatment, nothing else changed: agreement at python_fe's
    // tolerance
    assert_eq!(akantu_abnd, 0.0);
    assert!(akantu <= 1e-10, "{akantu:e} at step {akantu_step}");
}
