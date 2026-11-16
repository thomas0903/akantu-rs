// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Implicit cases of
// the benchmark suite; the problems follow
// examples/c++/solid_mechanics_model/static/static.cc,
// examples/c++/solid_mechanics_model/implicit/implicit_dynamic.cc and
// test/test_model/test_solid_mechanics_model/test_materials/
// test_material_elasto_plastic_linear_isotropic_hardening (load steps with
// Newton–Raphson).

//! Implicit cases: the linear static solve with each sparse direct solver,
//! a nonlinear static Newton–Raphson run with plasticity, and implicit
//! dynamics (trapezoidal rule).

use std::time::Instant;

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::{ElementType, Real, Result};
use akantu_linalg::direct::SparseSolverFactory;
use akantu_model::ModelOptions;
use akantu_parser::param::ParamValue;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::ModelSolverOptions;

use super::common::{self, StepSplit};
use super::mesh::{BoxSpec, box_mesh};
use crate::suite::{Ctx, Extra, Outcome};

/// Cells per axis of the implicit tetrahedron cube at sizes s, m, l.
const TET_N: [usize; 3] = [3, 14, 28];

fn options(
    method: AnalysisMethod,
    nls: Option<NonLinearSolverType>,
    sparse: SparseSolverType,
) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: nls,
        sparse_solver_type: Some(sparse),
        ..ModelSolverOptions::default()
    })
}

/// Clamp the face `z = 0` (all components) and block the z component of
/// the face `z = 1`, whose displacement is set to `uz`. Returns the top
/// face's z DOFs.
fn clamp_and_pull(m: &mut SolidMechanicsModel, uz: Real) -> Result<Vec<usize>> {
    let x = common::coordinates(m)?;
    let d = m.spatial_dimension();
    let tol = 1e-9;
    let bottom = common::face_nodes(&x, d, d - 1, 0.0, tol);
    let top = common::face_nodes(&x, d, d - 1, 1.0, tol);
    {
        let mut b = m.blocked_dofs_mut()?;
        let b = b.as_mut_slice();
        for &n in &bottom {
            b[n * d..n * d + d].fill(true);
        }
        for &n in &top {
            b[n * d + d - 1] = true;
        }
    }
    let dofs: Vec<usize> = top.iter().map(|&n| n * d + d - 1).collect();
    let mut u = m.displacement_mut()?;
    let u = u.as_mut_slice();
    for &k in &dofs {
        u[k] = uz;
    }
    Ok(dofs)
}

fn sparse_name(s: SparseSolverType) -> &'static str {
    match s {
        SparseSolverType::Faer => "faer",
        SparseSolverType::Mumps => "mumps",
        _ => "eigen",
    }
}

/// Linear static solve (`newton_raphson` replaced by the `linear`
/// non-linear solver: one assembly and one solve per step) of a clamped
/// cube pulled at its top, with the sparse solver `sparse`.
///
/// Phases: `mesh`, `init`, `solve_step` (first solve: assembly of `K`,
/// boundary conditions, analysis, factorisation, solve) with its
/// sub-phases, then `stiffness_assembly` (`assemble_stiffness_matrix`
/// with reassembly), and for the centralised solvers (Eigen's SparseLU
/// translation, faer) the direct solver driven on the assembled `J`:
/// `analysis` (ordering, symbolic), `factorize`, `back_substitution`.
///
/// # Errors
/// Model and solver errors.
pub fn static_linear(ctx: &mut Ctx<'_>, sparse: SparseSolverType) -> Result<Outcome> {
    if sparse == SparseSolverType::Mumps && !cfg!(feature = "mumps") {
        return Ok(Outcome::Skip("needs the `mumps` feature".into()));
    }
    // MUMPS works on an MPI communicator only (the binary passes the MPI
    // world; the in-process test passes a serial one)
    if sparse == SparseSolverType::Mumps && !ctx.comm().is_mpi() {
        return Ok(Outcome::Skip("needs the MPI world".into()));
    }
    let n = ctx.pick(TET_N[0], TET_N[1], TET_N[2]);
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    // MUMPS needs the MPI world: the cube is built on it (distributed at
    // P > 1); the centralised solvers run serially
    let mumps = sparse == SparseSolverType::Mumps;
    let mesh = if mumps {
        super::parallel::distributed_box(ctx, &spec)?
    } else {
        ctx.once("mesh", || box_mesh(&spec, None, true))?
    };
    let mut m = ctx.once("init", || {
        let mut m = common::solid_model(mesh, &common::elastic_dat(false))?;
        m.init_full(&options(
            AnalysisMethod::Static,
            Some(NonLinearSolverType::Linear),
            sparse,
        ))?;
        Ok(m)
    })?;
    clamp_and_pull(&mut m, 1e-3)?;
    // the first solve alone: assembly, analysis, factorisation, solve
    let mut split = StepSplit {
        on: true,
        ..StepSplit::default()
    };
    let t = Instant::now();
    m.solve_step_intercepted(&mut split, None)?;
    let first = t.elapsed().as_secs_f64();
    ctx.record("solve_step", 1, first);
    split.record(ctx, 1, first);
    let digest = if mumps {
        super::parallel::counts_and_digest(ctx, &m)?;
        None
    } else {
        common::set_counts(ctx, &m)?;
        Some(common::digest(&m)?)
    };
    let k = ctx.pick(1, 3, 3);
    ctx.repeat("stiffness_assembly", k, |_| {
        m.assemble_stiffness_matrix(true)
    })?;
    if !mumps {
        let factory = SparseSolverFactory::default();
        let mut solver = factory.create(sparse, 1)?;
        let backend = m.dof_manager().backend();
        let a = backend.aij("J")?;
        let rhs: Vec<Real> = (0..a.size()).map(|i| ((i % 7) as Real) - 3.0).collect();
        let mut x = vec![0.0; rhs.len()];
        ctx.once("analysis", || solver.analysis(a))?;
        ctx.repeat("factorize", ctx.pick(1, 2, 2), |_| solver.factorize(a))?;
        ctx.repeat("back_substitution", ctx.pick(2, 20, 20), |_| {
            solver.solve(&rhs, &mut x)
        })?;
        ctx.extra("nnz", Extra::Int(a.nnz() as i64));
    }
    ctx.extra("sparse_solver", Extra::Text(sparse_name(sparse).into()));
    if let Some(d) = digest {
        ctx.set_digest(&d);
    }
    Ok(Outcome::Done)
}

/// Static solve, Eigen's SparseLU translation (the default solver).
///
/// # Errors
/// Model and solver errors.
pub fn static_linear_eigen(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    static_linear(ctx, SparseSolverType::Eigen)
}

/// Static solve, faer.
///
/// # Errors
/// Model and solver errors.
pub fn static_linear_faer(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    static_linear(ctx, SparseSolverType::Faer)
}

/// Static solve, MUMPS (`mumps` feature).
///
/// # Errors
/// Model and solver errors.
pub fn static_linear_mumps(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    static_linear(ctx, SparseSolverType::Mumps)
}

/// Nonlinear static Newton–Raphson with linear isotropic hardening
/// plasticity: the clamped cube pulled in load steps past the yield strain.
///
/// Phases: `mesh`, `init`, `solve_step` (one load step: Newton iterations,
/// each with the tangent assembly and a factorisation) with its
/// sub-phases (`matrix` = tangent stiffness, `residual` = internal forces,
/// `step_rest` = the linear solves and the convergence tests).
///
/// # Errors
/// Model and solver errors (a load step that does not converge).
pub fn static_newton_plastic(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    let n = ctx.pick(3, 10, 20);
    let steps = ctx.pick(3, 12, 12);
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let mut m = ctx.once("init", || {
        let mut m = common::solid_model(mesh, &super::explicit::plastic_dat())?;
        m.init_full(&options(
            AnalysisMethod::Static,
            Some(NonLinearSolverType::NewtonRaphson),
            SparseSolverType::Eigen,
        ))?;
        Ok(m)
    })?;
    common::set_counts(ctx, &m)?;
    {
        let nls = m.solver.non_linear_solver_mut(None)?;
        nls.set_param("max_iterations", ParamValue::Int(200))?;
        nls.set_param("threshold", ParamValue::Real(1e-7))?;
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))?;
    }
    // the yield strain is 50e6 / 70e9 ≈ 7e-4: total strain 4e-3 over all
    // load steps (warm-up included)
    let total = crate::suite::WARMUP_CALLS + steps;
    let du = 4e-3 / total as Real;
    let top = clamp_and_pull(&mut m, 0.0)?;
    let mut iterations = 0usize;
    let mut split = StepSplit::default();
    let mut step_seconds = 0.0;
    let calls = ctx.repeat_primary("solve_step", steps, |timed| {
        {
            let mut u = m.displacement_mut()?;
            let u = u.as_mut_slice();
            for &k in &top {
                u[k] += du;
            }
        }
        split.on = timed;
        let t = Instant::now();
        m.solve_step_intercepted(&mut split, None)?;
        if timed {
            step_seconds += t.elapsed().as_secs_f64();
            iterations += m.solver.non_linear_solver(None)?.nb_iterations();
        }
        Ok(())
    })?;
    split.record(ctx, calls, step_seconds);
    ctx.extra("newton_iterations", Extra::Int(iterations as i64));
    ctx.set_digest(&common::digest(&m)?);
    Ok(Outcome::Done)
}

/// Implicit dynamics (trapezoidal rule, Newton–Raphson, Eigen's SparseLU
/// translation) of the pulse in a free cube, `Δt = 4 Δt_crit`.
///
/// Phases: `mesh`, `init`, `mass_consistent` (`assemble_mass`),
/// `solve_step` with its sub-phases (`matrix` = `K`, `M` and `J`,
/// `residual`, `predictor`, `corrector`, `step_rest` = the solves).
///
/// # Errors
/// Model and solver errors.
pub fn implicit_dynamic(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    let n = ctx.pick(TET_N[0], TET_N[1], TET_N[2]);
    let steps = ctx.pick(3, 60, 60);
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let mut m = ctx.once("init", || {
        let mut m = common::solid_model(mesh, &common::elastic_dat(false))?;
        m.init_full(&options(
            AnalysisMethod::ImplicitDynamic,
            None,
            SparseSolverType::Eigen,
        ))?;
        Ok(m)
    })?;
    common::set_counts(ctx, &m)?;
    ctx.once("mass_consistent", || m.assemble_mass())?;
    let dt = m.stable_time_step()?;
    m.set_time_step(4.0 * dt, None)?;
    common::apply_pulse(&mut m, 0.0, 1.0)?;
    common::timed_steps(ctx, &mut m, steps, |_| Ok(()))?;
    ctx.set_digest(&common::digest(&m)?);
    Ok(Outcome::Done)
}
