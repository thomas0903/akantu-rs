// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS):
// test/test_model/test_common/test_model_solver/{test_model_solver.cc,
// test_model_solver_dynamic.cc} (drivers) and the parametrised drivers of
// oracle/probes/solver_model/solver_model_probe.cc. Test support only.

//! Drivers replaying the oracle cases of `fixtures/solver_model/*` with the
//! Rust solver stack; each returns the same histories the probe records.

use akantu_comm::Communicator;
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Error, Real};
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, UpstreamCompat};
use akantu_validation::Fixture;

use super::my_model::{Law, MyModel, gen_mesh};
use super::oracle::{digest_arrays, f64s, param, param_str};

/// Every upstream-compatibility switch on: the oracle runs unpatched
/// upstream (UD-193 stale shared `J`, UD-194 `residual_mass_wgh`, UD-195
/// forward-Euler `K` read, UD-287 generalized-trapezoidal rate kept by the
/// predictor, UD-216 MUMPS retry right-hand side — no effect on the Eigen
/// solves of these drivers; the PETSc backend's UD-422/423/424/427/428,
/// ignored by the AIJ backend).
pub fn upstream_compat() -> UpstreamCompat {
    UpstreamCompat {
        stale_shared_jacobian: true,
        residual_mass_wgh_unrefreshed: true,
        generalized_trapezoidal_reads_k_when_e_is_zero: true,
        generalized_trapezoidal_keeps_rate_in_predictor: true,
        mumps_retry_with_solution_rhs: true,
        petsc_row_oriented_blocks: true,
        petsc_ignore_ksp_divergence: true,
        petsc_ksp_rhs_with_reactions: true,
        petsc_six_digit_real_options: true,
        petsc_aliased_solver_vectors: true,
        // UD-426's `J_petsc` dump is an I/O side effect without numerical
        // effect (the oracle runs need no file in their working directory).
        petsc_write_j_petsc: false,
        tao_bounds_excluding_zero_increment: true,
    }
}

/// Histories of one dynamic run (as the probe records them).
#[derive(Debug, Default)]
pub struct DynRun {
    pub energies: Vec<f64>,
    pub n_iter: Vec<i64>,
    pub error: Vec<f64>,
    pub converged: Vec<i64>,
    /// Normalised convergence threshold of each Newton step (the value the
    /// last test compared against).
    pub thresholds: Vec<f64>,
    pub threw: Vec<i64>,
    pub digests: Vec<String>,
    pub u_snap: Vec<f64>,
    pub v_snap: Vec<f64>,
    pub a_snap: Vec<f64>,
    pub hooks: String,
    pub initial_residual: Vec<f64>,
    pub failure: Option<(usize, Error)>,
    pub solver: Option<ModelSolver>,
}

pub fn scheme_type(name: &str) -> IntegrationSchemeType {
    name.parse().unwrap()
}

pub fn solution_type(name: &str) -> SolutionType {
    match name {
        "not_defined" => SolutionType::NotDefined,
        n => n.parse().unwrap(),
    }
}

fn record_energies(out: &mut Vec<f64>, u: &[f64], wext: Real, epot: Real, ekin: Real, einit: Real) {
    let etot = ekin + epot - wext - einit;
    let mut max_disp: Real = 0.0;
    let mut min_disp: Real = 0.0;
    for &d in u {
        // std::max / std::min of upstream (first argument kept on ties)
        if max_disp < d {
            max_disp = d;
        }
        if d < min_disp {
            min_disp = d;
        }
    }
    out.extend_from_slice(&[wext, epot, ekin, etot, max_disp, min_disp]);
}

/// The dynamic driver of `test_model_solver_dynamic.cc` (parametrised as in
/// the probe) on `f`'s inputs, with sparse solver `solver_name`.
pub fn run_dynamic(f: &Fixture, solver: SparseSolverType) -> DynRun {
    run_dynamic_on(f, solver, Communicator::serial())
}

/// [`run_dynamic`] on `comm` (a 1-process MPI communicator for the MUMPS
/// runs; the toy mesh is serial).
#[allow(dead_code)] // used by the mumps_mpi_solver binary only
pub fn run_dynamic_on(f: &Fixture, solver: SparseSolverType, comm: Communicator) -> DynRun {
    run_dynamic_with(
        f,
        solver,
        DofManager::new("model_solver:dof_manager_default", comm),
    )
}

/// [`run_dynamic`] with a given (empty) DOF manager, e.g. one with the
/// PETSc backend; the toy mesh is serial.
pub fn run_dynamic_with(f: &Fixture, solver: SparseSolverType, dm: DofManager) -> DynRun {
    let n = param(f, "nb_nodes").as_u64().unwrap() as usize;
    let steps = param(f, "max_steps").as_u64().unwrap() as usize;
    let dt = param(f, "time_step").as_f64().unwrap();
    let force = param(f, "force").as_f64().unwrap();
    let lumped_tss = param_str(f, "time_step_solver_type") == "dynamic_lumped";
    let scheme = scheme_type(&param_str(f, "integration_scheme"));
    let st = solution_type(&param_str(f, "solution_type"));
    let max_iterations = param(f, "max_iterations").as_i64().unwrap();
    let nonlinear = param(f, "nonlinear_bar").as_bool().unwrap();
    let snap_steps: Vec<u64> = param(f, "snapshot_steps")
        .as_array()
        .map(|a| a.iter().map(|x| x.as_u64().unwrap()).collect())
        .unwrap_or_default();
    let init_u = f64s(f, "inputs/initial_displacement");
    let init_v = f64s(f, "inputs/initial_velocity");
    let border = f64s(f, "inputs/border_bc");

    let mut dm = dm;
    dm.set_upstream_compat(upstream_compat());
    let mut model = MyModel::new(force, gen_mesh(n), lumped_tss, &mut dm).unwrap();
    if nonlinear {
        model.law = Law::Cubic;
    }
    // solver_model probe v3: DampedBar (Rayleigh C = a M + b K)
    if let Some(r) = f.manifest().parameters.get("rayleigh") {
        model.rayleigh = Some((
            r["mass"].as_f64().unwrap(),
            r["stiffness"].as_f64().unwrap(),
        ));
    }
    model.forces.iter_mut().for_each(|x| *x = 0.0);
    {
        let mut b = dm.blocked_dofs_mut("disp").unwrap();
        let b = b.as_mut_slice();
        b.iter_mut().for_each(|x| *x = false);
        b[0] = true;
        b[n - 1] = true;
    }
    dm.dofs_mut("disp")
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&init_u);
    dm.dofs_derivative_mut("disp", 1)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&init_v);

    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(if lumped_tss {
            TimeStepSolverType::DynamicLumped
        } else {
            TimeStepSolverType::Dynamic
        }),
        non_linear_solver_type: Some(if lumped_tss {
            NonLinearSolverType::Lumped
        } else {
            NonLinearSolverType::NewtonRaphson
        }),
        sparse_solver_type: Some(solver),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("dynamic", &opts, &mut model).unwrap();
    if !lumped_tss {
        // D-040: the resolved kind is the one installed (no substitution)
        assert_eq!(
            ms.dof_manager.backend().sparse_solver_kind("J"),
            Some(solver)
        );
    }
    ms.set_integration_scheme("dynamic", "disp", scheme, st, &model)
        .unwrap();
    if let Some(sp) = param(f, "scheme_parameters").as_object() {
        for (k, v) in sp {
            ms.solver_mut(Some("dynamic"))
                .unwrap()
                .integration_scheme_mut("disp")
                .unwrap()
                .set_param(k, ParamValue::Real(v.as_f64().unwrap()))
                .unwrap();
        }
    }
    ms.set_time_step(dt, Some("dynamic")).unwrap();

    let mut run = DynRun::default();
    let mut wext = 0.0;
    ms.dof_manager.zero_residual();
    model.assemble_residual_now(&mut ms.dof_manager).unwrap();
    run.initial_residual = ms.dof_manager.residual().values().to_vec();
    let (epot, ekin) = (0.0, 0.0);
    let einit = ekin + epot;
    record_energies(
        &mut run.energies,
        ms.dof_manager.dofs("disp").unwrap().as_slice(),
        wext,
        epot,
        ekin,
        0.0,
    );
    snapshot(&mut run, &ms.dof_manager);
    if !lumped_tss {
        ms.non_linear_solver_mut(Some("dynamic"))
            .unwrap()
            .set_param("max_iterations", ParamValue::Int(max_iterations))
            .unwrap();
    }
    for i in 1..=steps {
        {
            let dm = &mut ms.dof_manager;
            let b = &border[(i - 1) * 4..i * 4];
            let mut u_g = dm.dofs_mut("disp").unwrap();
            let u = u_g.as_mut_slice();
            u[0] = b[0];
            u[n - 1] = b[2];
            drop(u_g);
            let mut v = dm.dofs_derivative_mut("disp", 1).unwrap();
            let v = v.as_mut_slice();
            v[0] = b[1];
            v[n - 1] = b[3];
        }
        model.trace.clear();
        model.tracing = i <= 2;
        let threw = match ms.solve_step(&mut model, Some("dynamic")) {
            Ok(()) => false,
            Err(Error::NotConverged { .. }) => true,
            Err(e) => {
                run.failure = Some((i, e));
                break;
            }
        };
        run.threw.push(i64::from(threw));
        if i <= 2 {
            run.hooks += &format!("step {i}\n{}", model.trace_text());
        }
        let dm = &mut ms.dof_manager;
        let epot = if lumped_tss || dm.has_matrix("K") {
            model.potential_energy(dm).unwrap()
        } else {
            0.0
        };
        let ekin = model.kinetic_energy(dm).unwrap();
        wext += model.external_work_increment(dm, dt).unwrap();
        record_energies(
            &mut run.energies,
            dm.dofs("disp").unwrap().as_slice(),
            wext,
            epot,
            ekin,
            einit,
        );
        if !lumped_tss {
            let nls = ms.non_linear_solver_mut(Some("dynamic")).unwrap();
            run.n_iter.push(nls.nb_iterations() as i64);
            run.error.push(nls.error());
            run.converged.push(i64::from(nls.converged()));
            if let Some(nr) = nls.as_newton_raphson_mut() {
                run.thresholds.push(nr.normalized_threshold());
            }
        }
        let dm = &ms.dof_manager;
        run.digests.push(digest_arrays(&[
            dm.dofs("disp").unwrap().as_slice(),
            dm.dofs_derivative("disp", 1).unwrap().as_slice(),
            dm.dofs_derivative("disp", 2).unwrap().as_slice(),
        ]));
        if snap_steps.contains(&(i as u64)) {
            snapshot(&mut run, dm);
        }
        if threw {
            break;
        }
    }
    run.solver = Some(ms);
    run
}

fn snapshot(run: &mut DynRun, dm: &DofManager) {
    run.u_snap
        .extend_from_slice(dm.dofs("disp").unwrap().as_slice());
    run.v_snap
        .extend_from_slice(dm.dofs_derivative("disp", 1).unwrap().as_slice());
    run.a_snap
        .extend_from_slice(dm.dofs_derivative("disp", 2).unwrap().as_slice());
}

impl MyModel {
    /// `model.assembleResidual()` called directly by a driver (outside a
    /// solve): on a distributed bar the shared-node completion needs a
    /// collective operation.
    pub fn assemble_residual_now(&mut self, dm: &mut DofManager) -> akantu_core::Result<()> {
        use akantu_solver::SolverCallback;
        if dm.communicator().size() > 1 {
            dm.collective("assemble_residual", |dm| self.assemble_residual(dm))
        } else {
            self.assemble_residual(dm)
        }
    }
}

/// Histories of one non-linear static run.
#[derive(Debug, Default)]
pub struct NlRun {
    pub n_iter: Vec<i64>,
    pub error: Vec<f64>,
    pub converged: Vec<i64>,
    /// Normalised convergence threshold of each Newton step.
    pub thresholds: Vec<f64>,
    pub threw: Vec<i64>,
    pub u: Vec<f64>,
    pub r: Vec<f64>,
    pub hooks: String,
    pub solver: Option<ModelSolver>,
}

/// The probe's non-linear static driver (11-node cubic bar, pseudo-time).
pub fn run_nl_static(f: &Fixture, solver: SparseSolverType, legacy_hooks: bool) -> NlRun {
    let n = param(f, "nb_nodes").as_u64().unwrap() as usize;
    let df = param(f, "load_increment").as_f64().unwrap();
    let load_steps = param(f, "load_steps").as_i64().unwrap();
    let repeat = param(f, "repeat_last").as_bool().unwrap();
    let nls_type: NonLinearSolverType = param_str(f, "non_linear_solver_type").parse().unwrap();
    let criterion = param_str(f, "convergence_type");
    let max_iterations = param(f, "max_iterations").as_i64().unwrap();

    let mut dm = DofManager::new("model_solver:dof_manager_default", Communicator::serial());
    dm.set_upstream_compat(upstream_compat());
    let mut model = MyModel::new(0.0, gen_mesh(n), false, &mut dm).unwrap();
    model.law = Law::Cubic;
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(nls_type),
        sparse_solver_type: Some(solver),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("static", &opts, &mut model).unwrap();
    assert_eq!(
        ms.dof_manager.backend().sparse_solver_kind("J"),
        Some(solver)
    );
    ms.set_integration_scheme(
        "static",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &model,
    )
    .unwrap();
    let newton = nls_type != NonLinearSolverType::Linear;
    {
        let nls = ms.non_linear_solver_mut(None).unwrap();
        if newton {
            nls.set_param("max_iterations", ParamValue::Int(max_iterations))
                .unwrap();
            nls.set_param("convergence_type", ParamValue::Enum(criterion))
                .unwrap();
        }
        if let Some(nr) = nls.as_newton_raphson_mut() {
            nr.set_legacy_hooks(legacy_hooks);
        }
    }
    let mut run = NlRun::default();
    let total = load_steps + i64::from(repeat);
    for s in 1..=total {
        let load = s.min(load_steps);
        model.forces[n - 1] = df * load as f64;
        model.trace.clear();
        let threw = match ms.solve_step(&mut model, None) {
            Ok(()) => false,
            Err(Error::NotConverged { .. }) => true,
            Err(e) => panic!("{}: step {s}: {e}", f.case_id()),
        };
        run.hooks += &format!("step {s}\n{}", model.trace_text());
        run.threw.push(i64::from(threw));
        if newton {
            let nls = ms.non_linear_solver_mut(None).unwrap();
            run.n_iter.push(nls.nb_iterations() as i64);
            run.error.push(nls.error());
            run.converged.push(i64::from(nls.converged()));
            if let Some(nr) = nls.as_newton_raphson_mut() {
                run.thresholds.push(nr.normalized_threshold());
            }
        }
        run.u
            .extend_from_slice(ms.dof_manager.dofs("disp").unwrap().as_slice());
        run.r.extend_from_slice(&ms.dof_manager.residual().values());
        if threw {
            break;
        }
    }
    run.solver = Some(ms);
    run
}
