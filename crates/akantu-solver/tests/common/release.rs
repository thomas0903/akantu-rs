// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the static solve of
// test/test_model/test_common/test_model_solver/test_model_solver.cc on the
// toy bar (test_model_solver_my_model.hh). The release of the factorisation
// between two solves is a port extension (no upstream counterpart).

//! Release of the factorisation and of the matrices between two static
//! solves (`DofManager::release_factorization` / `release_matrix`): the
//! second solve analyses, factorises and assembles again and gives the
//! bits of a run that kept everything.

use akantu_comm::Communicator;
use akantu_core::Real;
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};

use super::my_model::{MyModel, gen_mesh};

/// Displacements after two static solves (the second with a doubled
/// load); `release`: release `J`'s factorisation and the storage of `J`
/// and `K` between them. Also returns the stored entries of `J` right
/// after the release (0 when released).
pub fn two_solves(
    comm: &Communicator,
    sparse: SparseSolverType,
    release: bool,
) -> (Vec<Real>, usize) {
    let mesh = gen_mesh(41);
    let mut dm = DofManager::new("dm", comm.clone());
    let mut model = MyModel::new(-10.0, mesh, false, &mut dm).expect("model");
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::Linear),
        sparse_solver_type: Some(sparse),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &opts, &mut model).expect("solver");
    ms.set_integration_scheme(
        "s",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &model,
    )
    .expect("scheme");
    ms.solve_step(&mut model, None).expect("first solve");
    let mut nnz_after = usize::MAX;
    if release {
        ms.dof_manager
            .release_factorization("J")
            .expect("release J factors");
        ms.dof_manager.release_matrix("J").expect("release J");
        ms.dof_manager.release_matrix("K").expect("release K");
        model.release_stiffness();
        nnz_after = ms.dof_manager.matrix("J").map(|m| m.nnz()).unwrap_or(0)
            + ms.dof_manager.matrix("K").map(|m| m.nnz()).unwrap_or(0);
    }
    for f in &mut model.forces {
        *f *= 2.0;
    }
    ms.solve_step(&mut model, None).expect("second solve");
    let u = ms
        .dof_manager
        .dofs("disp")
        .expect("disp")
        .as_slice()
        .to_vec();
    (u, nnz_after)
}

/// `Ok` if the run with releases gives the bits of the run without.
pub fn release_keeps_the_bits(comm: &Communicator, sparse: SparseSolverType) -> Result<(), String> {
    let (kept, _) = two_solves(comm, sparse, false);
    let (released, nnz) = two_solves(comm, sparse, true);
    if nnz != 0 {
        return Err(format!("{sparse:?}: {nnz} entries left after the release"));
    }
    let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    if bits(&kept) != bits(&released) {
        return Err(format!(
            "{sparse:?}: the second solve after the release differs"
        ));
    }
    if kept.iter().all(|&x| x == 0.0) {
        return Err(format!("{sparse:?}: trivial solution"));
    }
    Ok(())
}
