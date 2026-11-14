// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_linear_solver/
// non_linear_solver_tao.{hh,cc} (setBounds on the increment, TaoSolve,
// TAONotConvergedException) and the bound construction of
// test/test_model/test_phase_field_model/test_phase_field_tao.cc
// (`assembleToGlobalArray(dof, dofs, bound, -1.)`), on the partitioned
// MyModel toy of test/test_model/test_common/test_model_solver.

//! NLS-007: the first bounded TAO solves, at 1 / 2 / 4 ranks.
//!
//! The toy bar (17 nodes, `E = A = 1`, `h = 1/16`, node 0 fixed) carries a
//! nodal load `q` on every node but node 0, so its stiffness and load are
//! known in closed form (`K = 16·tridiag(−1, 2, −1)`, last diagonal 16).
//! Step 1: `q = −1`, unbounded `petsc_tao`; step 2: `q = −2`, the increment
//! bounded with upstream's construction (`lower = u_min`, `upper = 10³`,
//! then `assemble_to_global_array("disp", u, bound, −1)`, i.e.
//! `u + δ ≥ u_min`), so step 2 solves the discrete obstacle problem
//! `min ½uᵀKu − fᵀu, u ≥ u_min`. Checked against an independent dense
//! primal–dual active-set solve of that problem (rank 0) and its KKT
//! conditions, within rule PETSC-SNES (`1e-10` relative: a native PETSc
//! solver at its converged tolerance against a direct serial reference).
//!
//! `u_min = −12` lies below the step-1 solution (≥ −8.5), so `δ = 0` is
//! feasible, as in upstream's phase-field use (`0 ≤ δ ≤ 1 − d`): upstream's
//! TAO objective is exact only for a start point `δ = 0` (UD-1022).

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::{Error, Idx, Real};
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolver;

use crate::common::my_model::MyModel;
use crate::{KspConf, N, Outcome, Setup, build, e2s, gather_by_id};

/// The obstacle of step 2.
const U_MIN: Real = -12.0;

/// Nodal load `q` on the local-or-master nodes but node 0 (a shared node is
/// loaded once).
fn set_load(model: &mut MyModel, q: Real) -> Outcome {
    for n in 0..model.forces.len() {
        let gid = model.global_ids[n];
        let flag = model
            .mesh
            .node_flag(Idx::try_from(n).map_err(|e| e.to_string())?);
        model.forces[n] = if gid > 0 && flag.is_local_or_master() {
            q
        } else {
            0.0
        };
    }
    Ok(())
}

/// The TAO toy with the nodal load `−1`.
pub(crate) fn bounded_bar(c: &Communicator) -> Result<(MyModel, ModelSolver), String> {
    let (mut model, ms) = build(c, Setup::TaoLinear, true, KspConf::Direct)?;
    set_load(&mut model, -1.0)?;
    Ok((model, ms))
}

/// `(K, f)` of the toy with nodal load `q` on the free DOFs `1..N` (closed
/// form).
fn closed_form(q: Real) -> (Vec<Vec<Real>>, Vec<Real>) {
    let m = N - 1;
    let k = 16.0;
    let mut a = vec![vec![0.0; m]; m];
    for i in 0..m {
        a[i][i] = if i + 1 == m { k } else { 2.0 * k };
        if i + 1 < m {
            a[i][i + 1] = -k;
            a[i + 1][i] = -k;
        }
    }
    (a, vec![q; m])
}

/// Dense Gaussian elimination with partial pivoting (`a x = b`).
fn solve_dense(mut a: Vec<Vec<Real>>, mut b: Vec<Real>) -> Result<Vec<Real>, String> {
    let n = b.len();
    for col in 0..n {
        let p = (col..n)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .ok_or("empty system")?;
        if a[p][col] == 0.0 {
            return Err("singular system".into());
        }
        a.swap(col, p);
        b.swap(col, p);
        let pivot_row = a[col].clone();
        for r in col + 1..n {
            let f = a[r][col] / pivot_row[col];
            for (x, &v) in a[r].iter_mut().zip(&pivot_row).skip(col) {
                *x -= f * v;
            }
            let v = b[col];
            b[r] -= f * v;
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for cc in r + 1..n {
            s -= a[r][cc] * x[cc];
        }
        x[r] = s / a[r][r];
    }
    Ok(x)
}

/// `min ½uᵀKu − fᵀu, u ≥ l` by the primal–dual active-set method
/// (Hintermüller–Ito–Kunisch; finite for an M-matrix): `(u, active)`.
fn obstacle(a: &[Vec<Real>], f: &[Real], l: Real) -> Result<(Vec<Real>, Vec<bool>), String> {
    let n = f.len();
    let mut lambda = vec![0.0; n];
    let mut u = vec![0.0; n];
    let mut active = vec![false; n];
    for _ in 0..4 * n + 4 {
        let next: Vec<bool> = (0..n).map(|i| lambda[i] + (l - u[i]) > 0.0).collect();
        if next == active && lambda.iter().any(|&x| x != 0.0) {
            return Ok((u, active));
        }
        active = next;
        let free: Vec<usize> = (0..n).filter(|&i| !active[i]).collect();
        let mut sub = vec![vec![0.0; free.len()]; free.len()];
        let mut rhs = vec![0.0; free.len()];
        for (r, &i) in free.iter().enumerate() {
            rhs[r] = f[i];
            for j in 0..n {
                if active[j] {
                    rhs[r] -= a[i][j] * l;
                }
            }
            for (cc, &j) in free.iter().enumerate() {
                sub[r][cc] = a[i][j];
            }
        }
        let uf = solve_dense(sub, rhs)?;
        u.fill(l);
        for (r, &i) in free.iter().enumerate() {
            u[i] = uf[r];
        }
        for i in 0..n {
            let g: Real = (0..n).map(|j| a[i][j] * u[j]).sum::<Real>() - f[i];
            lambda[i] = if active[i] { g } else { 0.0 };
        }
        if free.len() == n {
            // unconstrained solution feasible: done
            if u.iter().all(|&x| x >= l) {
                return Ok((u, active));
            }
        }
    }
    Err("active-set iteration did not settle".into())
}

/// The final `disp` of the model gathered on rank 0 by global id.
fn gathered_u(
    c: &Communicator,
    model: &MyModel,
    ms: &ModelSolver,
) -> Result<Vec<(i64, Real)>, String> {
    let u = ms
        .dof_manager
        .dofs("disp")
        .map_err(e2s)?
        .as_slice()
        .to_vec();
    Ok(gather_by_id(c, &model.global_ids, &u))
}

/// Bounds of the next increment: `u + δ ∈ [lo, hi]` built as upstream's
/// test does (constant vectors, then `assemble_to_global_array(dof, u,
/// bound, −1)`), owned rows used by the solver.
fn increment_bounds(ms: &mut ModelSolver, lo: Real, hi: Real) -> Result<(), String> {
    let n = ms.dof_manager.local_system_size();
    let u = ms
        .dof_manager
        .dofs("disp")
        .map_err(e2s)?
        .as_slice()
        .to_vec();
    let mut lower = vec![lo; n];
    let mut upper = vec![hi; n];
    ms.dof_manager
        .assemble_to_global_array("disp", &u, &mut lower, -1.0)
        .map_err(e2s)?;
    ms.dof_manager
        .assemble_to_global_array("disp", &u, &mut upper, -1.0)
        .map_err(e2s)?;
    ms.non_linear_solver_mut(None)
        .map_err(e2s)?
        .as_native_mut()
        .ok_or("not a native solver")?
        .set_bounds(lower, upper)
        .map_err(e2s)
}

/// Prepare the bounded step 2 of [`petsc_tao_bounds`]: load `−2`, bounds
/// `u + δ ≥ U_MIN`.
pub(crate) fn bounded_step(model: &mut MyModel, ms: &mut ModelSolver) -> Outcome {
    set_load(model, -2.0)?;
    increment_bounds(ms, U_MIN, 1e3)
}

/// Max relative difference of gathered `u` (by id `1..N`) and `want`.
fn rel_diff(got: &[(i64, Real)], want: &[Real]) -> Real {
    let scale = want.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    got.iter()
        .filter(|(g, _)| *g > 0)
        .map(|(g, x)| {
            let i = usize::try_from(*g - 1).unwrap_or(usize::MAX);
            want.get(i)
                .map_or(Real::INFINITY, |w| (x - w).abs() / scale)
        })
        .fold(0.0, Real::max)
}

/// An unbounded then a bounded TAO step (see the module doc).
pub fn petsc_tao_bounds(c: &Communicator) -> Outcome {
    let (mut model, mut ms) = bounded_bar(c)?;
    let (a1, f1) = closed_form(-1.0);
    let (a, f) = closed_form(-2.0);
    let mut fails = Vec::new();
    // Step 1: q = −1, unbounded.
    ms.solve_step(&mut model, None).map_err(e2s)?;
    let it1 = ms.non_linear_solver(None).map_err(e2s)?.nb_iterations();
    let u1 = gathered_u(c, &model, &ms)?;
    // Step 2: q = −2, u + δ ≥ U_MIN (and ≤ 10³).
    set_load(&mut model, -2.0)?;
    increment_bounds(&mut ms, U_MIN, 1e3)?;
    ms.solve_step(&mut model, None).map_err(e2s)?;
    let nls = ms.non_linear_solver(None).map_err(e2s)?;
    let it2 = nls.nb_iterations();
    let gnorm = nls.error();
    let u2 = gathered_u(c, &model, &ms)?;
    if c.rank() == 0 {
        let free = solve_dense(a1, f1)?;
        let (obs, active) = obstacle(&a, &f, U_MIN)?;
        let r1 = rel_diff(&u1, &free);
        let r2 = rel_diff(&u2, &obs);
        // KKT of the TAO result with the closed-form K, f.
        let mut u = [0.0; N - 1];
        for (g, x) in &u2 {
            if *g > 0 {
                u[usize::try_from(*g - 1).map_err(|e| e.to_string())?] = *x;
            }
        }
        let grad: Vec<Real> = (0..N - 1)
            .map(|i| (0..N - 1).map(|j| a[i][j] * u[j]).sum::<Real>() - f[i])
            .collect();
        let mut kkt = 0.0_f64;
        let mut at_bound = 0.0_f64;
        for i in 0..N - 1 {
            if active[i] {
                at_bound = at_bound.max((u[i] - U_MIN).abs());
                kkt = kkt.max((-grad[i]).max(0.0));
            } else {
                kkt = kkt.max(grad[i].abs());
            }
            if u[i] < U_MIN - 1e-14 * U_MIN.abs() {
                fails.push(format!("node {}: {} violates the bound", i + 1, u[i]));
            }
        }
        let n_active = active.iter().filter(|&&x| x).count();
        println!(
            "INFO petsc_tao_bounds: ranks={} step 1 iterations {it1}, max rel diff {r1:e} vs \
             K^-1 f; step 2 iterations {it2}, final gradient norm {gnorm:e}, {n_active} active \
             nodes, max rel diff {r2:e} vs the active-set solution, KKT residual {kkt:e}, \
             max |u - u_min| at active nodes {at_bound:e}",
            c.size()
        );
        if r1.is_nan() || r1 > 1e-10 {
            fails.push(format!("step 1: {r1:e} from K^-1 f"));
        }
        if r2.is_nan() || r2 > 1e-10 {
            fails.push(format!("step 2: {r2:e} from the obstacle solution"));
        }
        let kkt_ok = kkt <= 1e-9 && at_bound <= 1e-14 * U_MIN.abs();
        if !kkt_ok || n_active == 0 {
            fails.push(format!(
                "step 2: KKT residual {kkt:e}, |u - u_min| {at_bound:e}, {n_active} active"
            ));
        }
    }
    agree(c, fails)
}

/// A TAO solve that stops at `tao_max_it` reports
/// `Error::NotConverged { threshold: gatol, niter, error: final gradient
/// norm }` on every rank, and upstream's `TAONotConvergedException` text
/// from the report: the bounded step of [`petsc_tao_bounds`] (4 GPCG
/// iterations) with `tao_max_it 1` (PETSc refuses 0).
pub fn petsc_tao_not_converged(c: &Communicator) -> Outcome {
    let (mut model, mut ms) = bounded_bar(c)?;
    ms.solve_step(&mut model, None).map_err(e2s)?;
    set_load(&mut model, -2.0)?;
    increment_bounds(&mut ms, U_MIN, 1e3)?;
    ms.non_linear_solver_mut(None)
        .map_err(e2s)?
        .set_param("tao_max_it", ParamValue::Int(1))
        .map_err(e2s)?;
    let r = ms.solve_step(&mut model, None);
    let nls = ms.non_linear_solver_mut(None).map_err(e2s)?;
    let report = nls
        .as_native_mut()
        .and_then(|n| n.report().cloned())
        .ok_or("no TAO report")?;
    let mut fails = Vec::new();
    match &r {
        Err(Error::NotConverged {
            threshold,
            niter,
            error,
        }) => {
            if *threshold != 1e-12
                || *niter != 1
                || error.is_nan()
                || *error <= 1e-12
                || *error != report.function_norm
            {
                fails.push(format!(
                    "NotConverged {{ threshold: {threshold:e}, niter: {niter}, error: {error:e} }} \
                     vs gatol 1e-12, 1 iteration, gradient norm {:e}",
                    report.function_norm
                ));
            }
        }
        other => fails.push(format!("expected NotConverged, got {other:?}")),
    }
    let msg = report.tao_not_converged_message(1);
    let want = format!(
        "The TAO solver did not converge after 1 iterations (reason: {}, max_iterations: 1, atol: \
         1e-12, rtol: 0, ttol: {})",
        report.reason,
        akantu_linalg::matrix_market::format_g(report.stol, 6)
    );
    // TAO_DIVERGED_MAXITS = -2
    if report.reason != -2 || report.max_iterations != 1 || msg != want {
        fails.push(format!(
            "report: reason {} max_it {}; message {msg:?}",
            report.reason, report.max_iterations
        ));
    }
    if c.rank() == 0 {
        println!(
            "INFO petsc_tao_not_converged: ranks={} error {r:?}; {msg}",
            c.size()
        );
    }
    agree(c, fails)
}

/// The DOF manager's UD-1022 compat switch
/// (`tao_bounds_excluding_zero_increment`, D-088 (1)).
fn ud1022_compat(ms: &mut ModelSolver, on: bool) {
    let mut compat = ms.dof_manager.upstream_compat();
    compat.tao_bounds_excluding_zero_increment = on;
    ms.dof_manager.set_upstream_compat(compat);
}

/// UD-1022 evidence, **under the compat switch**
/// (`tao_bounds_excluding_zero_increment`; by default such bounds are
/// refused, D-088 (1): [`petsc_tao_ud1022_refused`]): upstream's TAO
/// objective (`computeObjectiveGradient`: residual at the trial point `δ`,
/// plus `J δ` again) is exact only at `δ = 0`; GPCG builds its quadratic
/// model from the gradient at its start point, which it projects onto the
/// bounds. With a bound that excludes `δ = 0` (`u + δ ≥ −4` after step 1,
/// whose solution reaches −8.5) the start is `δ₀ = max(lower, 0) ≠ 0`, the
/// model's linear term is `K(u + δ₀) − f` instead of `K u − f`, and GPCG
/// returns the obstacle solution for the load `f − K δ₀`. Checked: the
/// result differs from the true obstacle solution (load `f`) and equals the
/// predicted one within rule PETSC-SNES. The port reproduces upstream's
/// objective (bitwise against the oracle in the TAO cases), so the switch
/// reproduces upstream's result; upstream's own use (phase field,
/// `0 ≤ δ ≤ 1 − d`) always contains `δ = 0`.
pub fn petsc_tao_ud1022(c: &Communicator) -> Outcome {
    const U2: Real = -4.0;
    let (mut model, mut ms) = bounded_bar(c)?;
    ud1022_compat(&mut ms, true);
    let (a, f) = closed_form(-1.0);
    ms.solve_step(&mut model, None).map_err(e2s)?;
    let u1 = gathered_u(c, &model, &ms)?;
    increment_bounds(&mut ms, U2, 1e3)?;
    ms.solve_step(&mut model, None).map_err(e2s)?;
    let u2 = gathered_u(c, &model, &ms)?;
    let mut fails = Vec::new();
    if c.rank() == 0 {
        let m = N - 1;
        let mut u1v = vec![0.0; m];
        for (g, x) in &u1 {
            if *g > 0 {
                u1v[usize::try_from(*g - 1).map_err(|e| e.to_string())?] = *x;
            }
        }
        // GPCG's projected start point and the load its model sees.
        let x0: Vec<Real> = u1v.iter().map(|&u| (U2 - u).max(0.0)).collect();
        let kx0: Vec<Real> = (0..m)
            .map(|i| (0..m).map(|j| a[i][j] * x0[j]).sum::<Real>())
            .collect();
        let f_model: Vec<Real> = f.iter().zip(&kx0).map(|(fi, k)| fi - k).collect();
        let (truth, _) = obstacle(&a, &f, U2)?;
        let (predicted, _) = obstacle(&a, &f_model, U2)?;
        let r_truth = rel_diff(&u2, &truth);
        let r_pred = rel_diff(&u2, &predicted);
        let nonzero = x0.iter().filter(|&&x| x > 0.0).count();
        println!(
            "INFO petsc_tao_ud1022: ranks={} start point nonzero at {nonzero} nodes; result vs \
             the obstacle solution of f: max rel diff {r_truth:e}; vs the predicted one of \
             f - K x0: {r_pred:e}",
            c.size()
        );
        if nonzero == 0 || r_truth.is_nan() || r_truth <= 1e-6 {
            fails.push(format!(
                "UD-1022 not observed: {nonzero} nonzero start entries, {r_truth:e} from the \
                 true solution"
            ));
        }
        if r_pred.is_nan() || r_pred > 1e-10 {
            fails.push(format!(
                "predicted (f - K x0) solution missed by {r_pred:e}"
            ));
        }
    }
    agree(c, fails)
}

/// Digest of a text (identical texts on every rank: `max == min`).
fn text_digest(t: &str) -> u64 {
    akantu_comm::phase::digest_bytes(0xcbf2_9ce4_8422_2325, t.as_bytes())
}

/// The error of a refused solve: `Unsupported`, naming UD-1022 and `rows`
/// owned rows, the same text on every rank. `Err` describes a deviation.
fn check_refusal(
    c: &Communicator,
    what: &str,
    r: &akantu_core::Result<()>,
    rows: usize,
) -> Result<(), String> {
    let text = match r {
        Err(Error::Unsupported(m)) => m.clone(),
        other => {
            return Err(format!(
                "{what}: expected Unsupported (UD-1022), got {other:?}"
            ));
        }
    };
    let d = text_digest(&text);
    let mut v = [d, !d];
    c.all_reduce(&mut v, ReduceOp::Max).map_err(e2s)?;
    if v[0] != !v[1] {
        return Err(format!(
            "{what}: the refusal's text differs between the ranks: {text}"
        ));
    }
    let want = format!("on {rows} owned row(s) of the job");
    if !text.contains("UD-1022")
        || !text.contains(&want)
        || !text.contains("tao_bounds_excluding_zero_increment")
    {
        return Err(format!(
            "{what}: refusal text {text:?} (want UD-1022, '{want}')"
        ));
    }
    if c.rank() == 0 {
        println!("INFO petsc_tao_ud1022_refused: {what}: {text}");
    }
    Ok(())
}

/// D-088 (1), UD-1022 category A: by default a bounded TAO solve whose
/// bounds exclude `δ = 0` on some owned row (`lower > 0` or `upper < 0`) is
/// refused before anything of the solve runs, with the same
/// `Error::Unsupported` naming UD-1022 and the job's count of such rows on
/// every rank, and the refusal is recoverable. At 1 / 2 / 4 ranks:
///
/// 1. after the unbounded step 1, the bounds `u + δ ≥ −4` of
///    [`petsc_tao_ud1022`] (excluding `δ = 0` wherever `u < −4`) are
///    refused; the displacement is unchanged bit for bit;
/// 2. bounds that exclude `δ = 0` on **one** owned row of the last rank
///    only (`lower = 0.5` there, `−10³` elsewhere) are refused on every
///    rank with the count 1; NaN bounds (one owned row of the last rank,
///    one of rank 0; ±inf elsewhere) are refused the same way;
/// 3. recoverable: the bounded step 2 of [`petsc_tao_bounds`] (load −2,
///    `u + δ ≥ −12`) then solves and matches the independent active-set
///    solution within rule PETSC-SNES;
/// 4. at more than one rank, a bound excluding `δ = 0` on a **slave** row
///    only (not owned: never handed to TAO) is not refused.
pub fn petsc_tao_ud1022_refused(c: &Communicator) -> Outcome {
    let (mut model, mut ms) = bounded_bar(c)?;
    let mut fails = Vec::new();
    ms.solve_step(&mut model, None).map_err(e2s)?;
    let local_u = |ms: &ModelSolver| -> Result<Vec<Real>, String> {
        Ok(ms
            .dof_manager
            .dofs("disp")
            .map_err(e2s)?
            .as_slice()
            .to_vec())
    };
    let u1 = local_u(&ms)?;
    // 1. Bounds excluding δ = 0 wherever u < −4 (the job's count from the
    // gathered step-1 solution).
    increment_bounds(&mut ms, -4.0, 1e3)?;
    let excluded = {
        let g = gathered_u(c, &model, &ms)?;
        let mut n = [
            u64::try_from(g.iter().filter(|(_, u)| -4.0 - u > 0.0).count())
                .map_err(|e| e.to_string())?,
        ];
        // gathered on rank 0 only: broadcast the count
        c.all_reduce(&mut n, ReduceOp::Max).map_err(e2s)?;
        usize::try_from(n[0]).map_err(|e| e.to_string())?
    };
    let r = ms.solve_step(&mut model, None);
    if excluded == 0 {
        fails.push("case 1: no row excludes the zero increment".into());
    } else if let Err(e) = check_refusal(c, "case 1", &r, excluded) {
        fails.push(e);
    }
    let u_after = local_u(&ms)?;
    if u_after
        .iter()
        .zip(&u1)
        .any(|(a, b)| a.to_bits() != b.to_bits())
        || u_after.len() != u1.len()
    {
        fails.push("case 1: the refused solve changed the displacement".into());
    }
    // 2. One owned row of the last rank only.
    {
        let layout = ms.dof_manager.layout();
        let n = layout.nb_local();
        let mut lower = vec![-1e3; n];
        let upper = vec![1e3; n];
        if c.rank() + 1 == i32::try_from(c.size()).map_err(|e| e.to_string())? {
            let l = (0..n).find(|&l| layout.is_owned(l)).ok_or("no owned row")?;
            lower[l] = 0.5;
        }
        ms.non_linear_solver_mut(None)
            .map_err(e2s)?
            .as_native_mut()
            .ok_or("not a native solver")?
            .set_bounds(lower, upper)
            .map_err(e2s)?;
        let r = ms.solve_step(&mut model, None);
        if let Err(e) = check_refusal(c, "case 2", &r, 1) {
            fails.push(e);
        }
    }
    // 2b. A NaN bound: a NaN lower bound on one owned
    // row of the last rank and a NaN upper bound on one of rank 0 (the same
    // row at one rank), ±inf elsewhere (admitted): refused like case 2.
    {
        let layout = ms.dof_manager.layout();
        let n = layout.nb_local();
        let mut lower = vec![Real::NEG_INFINITY; n];
        let mut upper = vec![Real::INFINITY; n];
        let owned = (0..n).find(|&l| layout.is_owned(l)).ok_or("no owned row")?;
        if c.rank() + 1 == i32::try_from(c.size()).map_err(|e| e.to_string())? {
            lower[owned] = Real::NAN;
        }
        if c.rank() == 0 {
            upper[owned] = Real::NAN;
        }
        ms.non_linear_solver_mut(None)
            .map_err(e2s)?
            .as_native_mut()
            .ok_or("not a native solver")?
            .set_bounds(lower, upper)
            .map_err(e2s)?;
        let r = ms.solve_step(&mut model, None);
        let rows = if c.size() == 1 { 1 } else { 2 };
        if let Err(e) = check_refusal(c, "case 2b (NaN bounds)", &r, rows) {
            fails.push(e);
        }
    }
    // 3. Recoverable: the bounded step 2 of petsc_tao_bounds.
    bounded_step(&mut model, &mut ms)?;
    match ms.solve_step(&mut model, None) {
        Ok(()) => {
            let u2 = gathered_u(c, &model, &ms)?;
            if c.rank() == 0 {
                let (a, f) = closed_form(-2.0);
                let (obs, _) = obstacle(&a, &f, U_MIN)?;
                let r2 = rel_diff(&u2, &obs);
                println!(
                    "INFO petsc_tao_ud1022_refused: ranks={} case 3 (after two refusals): max rel \
                     diff {r2:e} vs the active-set solution",
                    c.size()
                );
                if r2.is_nan() || r2 > 1e-10 {
                    fails.push(format!("case 3: {r2:e} from the obstacle solution"));
                }
            }
        }
        Err(e) => fails.push(format!("case 3: the solve after the refusals failed: {e}")),
    }
    // 4. A slave row only (more than one rank).
    if c.size() > 1 {
        let layout = ms.dof_manager.layout();
        let n = layout.nb_local();
        let mut lower = vec![-1e3; n];
        let upper = vec![1e3; n];
        let slaves = (0..n).filter(|&l| !layout.is_owned(l)).count();
        if let Some(l) = (0..n).find(|&l| !layout.is_owned(l)) {
            lower[l] = 0.5;
        }
        let mut s = [u64::try_from(slaves).map_err(|e| e.to_string())?];
        c.all_reduce(&mut s, ReduceOp::Sum).map_err(e2s)?;
        ms.non_linear_solver_mut(None)
            .map_err(e2s)?
            .as_native_mut()
            .ok_or("not a native solver")?
            .set_bounds(lower, upper)
            .map_err(e2s)?;
        let r = ms.solve_step(&mut model, None);
        if c.rank() == 0 {
            println!(
                "INFO petsc_tao_ud1022_refused: case 4: {} slave rows in the job, a bound \
                 excluding 0 on one slave row per rank: {r:?}",
                s[0]
            );
        }
        if s[0] == 0 {
            fails.push("case 4: no slave row".into());
        } else if let Err(e) = r {
            fails.push(format!(
                "case 4: a slave-row bound was refused or failed: {e}"
            ));
        }
    }
    agree(c, fails)
}

fn agree(c: &Communicator, fails: Vec<String>) -> Outcome {
    let mut ok = [u8::from(fails.is_empty())];
    c.all_reduce(&mut ok, ReduceOp::Min).map_err(e2s)?;
    if !fails.is_empty() {
        return Err(fails.join("; "));
    }
    if ok[0] != 1 {
        return Err("failed on another rank".into());
    }
    Ok(())
}
