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
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Array, GlobalIdx, Real, Result};
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback};

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
        ms.dof_manager
            .release_matrix_values("K")
            .expect("release K's values");
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

/// A chain of `n` generic DOFs (DOF 0 blocked, a load on the last) whose
/// `K` is assembled through the backend like a contact term: two-DOF
/// blocks along the chain (unsymmetric with `promote`, which promotes the
/// symmetric `K` at its first assembly, UD-034) and a set of couplings
/// between distant DOFs that changes from solve to solve. A coupling that
/// is no longer assembled stays in the profile with a zero value, as a
/// contact pair that opened again.
pub struct Chain {
    pub n: usize,
    pub promote: bool,
    pub couplings: Vec<(usize, usize)>,
    pub assembled: bool,
    pub load: Real,
}

impl SolverCallback for Chain {
    fn matrix_type(&self, id: &str) -> MatrixType {
        if id == "K" {
            MatrixType::Symmetric
        } else {
            MatrixType::NotDefined
        }
    }
    fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        if id != "K" || self.assembled {
            return Ok(());
        }
        let leq: Vec<usize> = dm
            .local_equation_numbers("disp")?
            .iter()
            .map(|&l| l as usize)
            .collect();
        let g = |d: usize| -> GlobalIdx {
            dm.local_to_global_equation_number(leq[d])
                .expect("owned generic DOF")
        };
        let chain: Vec<[GlobalIdx; 2]> = (0..self.n - 1).map(|e| [g(e), g(e + 1)]).collect();
        let couplings: Vec<[GlobalIdx; 2]> =
            self.couplings.iter().map(|&(a, b)| [g(a), g(b)]).collect();
        dm.zero_matrix("K")?;
        let backend = dm.backend_mut()?;
        for (e, rows) in chain.iter().enumerate() {
            let s = 1.0 + 0.125 * e as Real;
            let (block, ty) = if self.promote {
                ([2.0 * s, -0.75 * s, -s, 2.0 * s], MatrixType::Unsymmetric)
            } else {
                ([2.0 * s, -s, -s, 2.0 * s], MatrixType::Symmetric)
            };
            backend.add_block_to_profile("K", rows, rows)?;
            backend.add_values("K", rows, rows, &block, ty)?;
        }
        for rows in &couplings {
            let block = [0.375, -0.375, -0.375, 0.375];
            backend.add_block_to_profile("K", rows, rows)?;
            backend.add_values("K", rows, rows, &block, MatrixType::Symmetric)?;
        }
        self.assembled = true;
        Ok(())
    }
    fn assemble_lumped_matrix(&mut self, _: &mut DofManager, _: &str) -> Result<()> {
        Ok(())
    }
    fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
        let mut f = vec![0.0; self.n];
        f[self.n - 1] = self.load;
        let fa = Array::from_vec(f, 1)?;
        dm.assemble_to_residual("disp", fa.view(), 1.0)
    }
}

/// What a run of [`chain_solves`] ends with: the displacements and the
/// coordinates and value bits of `K` and `J`.
#[derive(Debug, PartialEq, Eq)]
pub struct ChainState {
    pub u: Vec<u64>,
    pub k: (Vec<GlobalIdx>, Vec<GlobalIdx>, Vec<u64>),
    pub j: (Vec<GlobalIdx>, Vec<GlobalIdx>, Vec<u64>),
}

/// Four static solves of a [`Chain`] (couplings: none, then `(1, n-1)`,
/// then `(2, n-2)`, then `(2, n-2)` again: the profile grows, and the
/// first coupling stays in it with a zero value); `release`: the static
/// solver is released after the third solve exactly as
/// `SolidMechanicsModel::release_static_solver` does it (`J`'s
/// factorisation and storage, `K`'s values), so the fourth solve assembles,
/// analyses and factorises again.
pub fn chain_solves(
    comm: &Communicator,
    sparse: SparseSolverType,
    promote: bool,
    release: bool,
) -> std::result::Result<ChainState, String> {
    let e2s = |e: akantu_core::Error| e.to_string();
    let n = 24;
    let mut dm = DofManager::new("dm", comm.clone());
    dm.register_generic_dofs("disp", n, 1).map_err(e2s)?;
    dm.register_blocked_dofs("disp").map_err(e2s)?;
    dm.blocked_dofs_mut("disp").map_err(e2s)?.as_mut_slice()[0] = true;
    let mut cb = Chain {
        n,
        promote,
        couplings: Vec::new(),
        assembled: false,
        load: 10.0,
    };
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::Linear),
        sparse_solver_type: Some(sparse),
        ..ModelSolverOptions::default()
    };
    ms.new_solver("s", &opts).map_err(e2s)?;
    ms.set_integration_scheme(
        "s",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &cb,
    )
    .map_err(e2s)?;
    let schedule = [vec![], vec![(1, n - 1)], vec![(2, n - 2)], vec![(2, n - 2)]];
    for (step, couplings) in schedule.into_iter().enumerate() {
        if release && step == 3 {
            let dm = &mut ms.dof_manager;
            dm.release_factorization("J").map_err(e2s)?;
            dm.release_matrix("J").map_err(e2s)?;
            dm.release_matrix_values("K").map_err(e2s)?;
            if !dm.matrix("K").map_err(e2s)?.is_released()
                || dm.matrix("J").map_err(e2s)?.nnz() != 0
            {
                return Err("K or J not released".into());
            }
        }
        cb.couplings = couplings;
        cb.assembled = false;
        cb.load *= 1.5;
        ms.solve_step(&mut cb, None).map_err(e2s)?;
    }
    let dm = &ms.dof_manager;
    let state = |id: &str| -> std::result::Result<_, String> {
        let (i, j, a) = dm.matrix(id).map_err(e2s)?.coo();
        Ok((
            i.to_vec(),
            j.to_vec(),
            a.iter().map(|v| v.to_bits()).collect(),
        ))
    };
    if promote && dm.matrix("K").map_err(e2s)?.matrix_type() != MatrixType::Unsymmetric {
        return Err("K was not promoted".into());
    }
    Ok(ChainState {
        u: dm
            .dofs("disp")
            .map_err(e2s)?
            .as_slice()
            .iter()
            .map(|v| v.to_bits())
            .collect(),
        k: state("K")?,
        j: state("J")?,
    })
}

/// `Ok` if [`chain_solves`] with the release equals the run without it,
/// for a grown profile, with and without a promotion.
pub fn chain_release_keeps_the_bits(
    comm: &Communicator,
    sparse: SparseSolverType,
) -> std::result::Result<(), String> {
    for promote in [false, true] {
        let kept = chain_solves(comm, sparse, promote, false)?;
        let released = chain_solves(comm, sparse, promote, true)?;
        if kept.k.0 != released.k.0 || kept.k.1 != released.k.1 {
            return Err(format!(
                "{sparse:?}, promote {promote}: K's entry order differs"
            ));
        }
        if kept != released {
            return Err(format!(
                "{sparse:?}, promote {promote}: values or solution differ after the release"
            ));
        }
    }
    Ok(())
}
