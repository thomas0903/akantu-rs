// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test helper of the
// model-coupler tests: the counterpart of the couplers probe's
// `TraceCallback` (upstream `InterceptSolverCallback`,
// src/model/common/solver_callback.hh): forwards every call to the coupler's
// callback and reports the events after which the state is recorded.

use akantu_core::Result;
use akantu_core::enums::{MatrixType, TimeStepSolverType};
use akantu_couplers::SolidContactCallback;
use akantu_solver::{DofManager, ModelSolverOptions, SolverCallback};

/// A [`SolidContactCallback`] whose events `"matrix:<id>"`, `"residual"`
/// and `"corrector"` are reported to `on_event` after the forwarded call.
pub struct Traced<'a, 'b, F>
where
    F: FnMut(&str, &DofManager, &SolidContactCallback<'a>),
{
    /// The traced callback.
    pub inner: &'b mut SolidContactCallback<'a>,
    /// Event observer.
    pub on_event: F,
}

impl<'a, F> SolverCallback for Traced<'a, '_, F>
where
    F: FnMut(&str, &DofManager, &SolidContactCallback<'a>),
{
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        self.inner.matrix_type(matrix_id)
    }
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        self.inner.assemble_matrix(dofs, matrix_id)?;
        (self.on_event)(&format!("matrix:{matrix_id}"), dofs, self.inner);
        Ok(())
    }
    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        self.inner.assemble_lumped_matrix(dofs, matrix_id)
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.inner.assemble_residual(dofs)?;
        (self.on_event)("residual", dofs, self.inner);
        Ok(())
    }
    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        self.inner.assemble_residual_part(dofs, part)
    }
    fn can_split_residual(&self) -> bool {
        self.inner.can_split_residual()
    }
    fn predictor(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.inner.predictor(dofs)
    }
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.inner.corrector(dofs)?;
        (self.on_event)("corrector", dofs, self.inner);
        Ok(())
    }
    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.inner.before_solve_step(dofs)
    }
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        self.inner.after_solve_step(dofs, converged)
    }
    fn restore_last_converged_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.inner.restore_last_converged_step(dofs)
    }
    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        self.inner.init_solver(dofs, ty)
    }
    fn default_solver_type(&self) -> TimeStepSolverType {
        self.inner.default_solver_type()
    }
    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        self.inner.default_solver_options(ty)
    }
}

/// Triplets (0-based rows, 0-based columns, values) of a DOF manager matrix
/// in storage order.
pub fn triplets(dofs: &DofManager, id: &str) -> (Vec<i64>, Vec<i64>, Vec<f64>) {
    let k = dofs.matrix(id).unwrap();
    let irn = k.irn_one_based().iter().map(|&i| i - 1).collect();
    let jcn = k.jcn_one_based().iter().map(|&j| j - 1).collect();
    (irn, jcn, k.values().to_vec())
}
