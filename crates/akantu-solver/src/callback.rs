// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/solver_callback.hh,
// solver_callback.cc.

//! The solver callback (upstream `SolverCallback`).

use akantu_core::Result;
use akantu_core::enums::{MatrixType, NonLinearSolverType, TimeStepSolverType};

use crate::dof_manager::DofManager;
use crate::model_solver::ModelSolverOptions;

/// What the solvers ask of a physics model. Every method receives the DOF
/// manager explicitly (see crate docs for the ownership rationale).
///
/// Default implementations mirror upstream defaults; methods upstream marks
/// `AKANTU_TO_IMPLEMENT()` return `Error::NotImplemented`.
pub trait SolverCallback {
    /// Matrix type needed for `matrix_id` (`"K"`, `"M"`, `"C"`, `"J"`).
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        let _ = matrix_id;
        MatrixType::NotDefined
    }

    /// Assemble a sparse matrix into `dofs`.
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        let _ = (dofs, matrix_id);
        Err(akantu_core::Error::NotImplemented(
            "SolverCallback::assemble_matrix",
        ))
    }

    /// Assemble a lumped (diagonal) matrix.
    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        let _ = (dofs, matrix_id);
        Err(akantu_core::Error::NotImplemented(
            "SolverCallback::assemble_lumped_matrix",
        ))
    }

    /// Assemble the full residual (e.g. external − internal forces).
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        let _ = dofs;
        Err(akantu_core::Error::NotImplemented(
            "SolverCallback::assemble_residual",
        ))
    }

    /// Assemble one part of the residual (`"external"`, `"internal"`).
    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        let _ = (dofs, part);
        Ok(())
    }

    /// Whether the residual can be assembled in parts.
    fn can_split_residual(&self) -> bool {
        false
    }

    /// Dynamic predictor hook (called after the integration-scheme predictor).
    fn predictor(&mut self, dofs: &mut DofManager) -> Result<()> {
        let _ = dofs;
        Ok(())
    }

    /// Dynamic corrector hook (called after the integration-scheme corrector).
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        let _ = dofs;
        Ok(())
    }

    /// Before a solve step.
    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        let _ = dofs;
        Ok(())
    }

    /// After a solve step.
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        let _ = (dofs, converged);
        Ok(())
    }

    /// Restore the last converged state (upstream
    /// `restoreLastConvergedStep`).
    fn restore_last_converged_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        let _ = dofs;
        Ok(())
    }

    // ---- model-side virtuals of upstream `ModelSolver` (used by
    // `ModelSolver::get_new_solver` / `init_from_section`) -------------

    /// Instantiate what the model needs for a solver of type `ty` (upstream
    /// `ModelSolver::initSolver`, called by `getNewSolver`).
    fn init_solver(&mut self, dofs: &mut DofManager, ty: TimeStepSolverType) -> Result<()> {
        let _ = (dofs, ty);
        Ok(())
    }

    /// Default time-step solver type (upstream
    /// `ModelSolver::getDefaultSolverType`: `dynamic_lumped`).
    fn default_solver_type(&self) -> TimeStepSolverType {
        TimeStepSolverType::DynamicLumped
    }

    /// Default options for a solver of type `ty` (upstream
    /// `ModelSolver::getDefaultSolverOptions`: non-linear solver `auto`,
    /// everything else unset).
    fn default_solver_options(&self, ty: TimeStepSolverType) -> ModelSolverOptions {
        let _ = ty;
        ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::Auto),
            ..ModelSolverOptions::default()
        }
    }

    /// Downcasting hook (no upstream counterpart): a model callback that
    /// returns `Some(self)` lets an intercepting user callback reach the
    /// concrete model state through [`Delegate::split_as`] (upstream user
    /// callbacks simply keep a C++ reference to the model, e.g.
    /// `NTNContactSolverCallback::solid`). Default: `None`.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

/// The model callback as seen from an intercepting user callback: the
/// user's `super()` (upstream `InterceptSolverCallback::solver_callback`).
/// It only exists during one `solve_step_intercepted` call, so a foreign
/// callback can never call the model while the model is otherwise borrowed.
pub struct Delegate<'a> {
    inner: &'a mut dyn SolverCallback,
    dofs: &'a mut DofManager,
}

impl std::fmt::Debug for Delegate<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Delegate").finish_non_exhaustive()
    }
}

impl Delegate<'_> {
    /// DOF manager of the running solve.
    pub fn dofs(&mut self) -> &mut DofManager {
        self.dofs
    }
    /// The model callback as its concrete type `T` together with the DOF
    /// manager (`None` when the model callback is not a `T` or does not
    /// expose itself through [`SolverCallback::as_any_mut`]).
    ///
    /// # Usage contract
    /// This is how an intercepting callback reaches the model state that an
    /// upstream user callback reads through its own reference to the model
    /// (e.g. `NTNContactSolverCallback::solid`). The borrow lasts for one
    /// callback invocation only. Through it a callback may
    /// * read the model state (arrays, scalars, parameters), and
    /// * call the model's own refresh entry points that upstream's getters
    ///   run implicitly (e.g. `SolidMechanicsPhysics::refresh_current_position`,
    ///   upstream `getCurrentPosition()`).
    ///
    /// It must **not** change the model state otherwise: the steps of the
    /// solve (assembly, predictor, corrector, the before/after-step hooks)
    /// and every collective of the phase protocol (ARCHITECTURE) go
    /// through the delegate methods of this type, as upstream's
    /// `InterceptSolverCallback` forwards them to its `solver_callback`.
    /// Read-only users should prefer [`Delegate::split_as_ref`]. The DOF
    /// manager is the one [`Delegate::dofs`] already lends.
    pub fn split_as<T: 'static>(&mut self) -> Option<(&mut T, &mut DofManager)> {
        let dofs = &mut *self.dofs;
        let any = self.inner.as_any_mut()?;
        any.downcast_mut::<T>().map(|t| (t, dofs))
    }
    /// Shared-borrow variant of [`Delegate::split_as`] for read-only users:
    /// the model callback as `&T` and the DOF manager as `&DofManager`.
    pub fn split_as_ref<T: 'static>(&mut self) -> Option<(&T, &DofManager)> {
        let dofs = &*self.dofs;
        let any = self.inner.as_any_mut()?;
        any.downcast_ref::<T>().map(|t| (t, dofs))
    }
    /// Model's `matrix_type` (upstream `getMatrixType`; a Python
    /// intercepting callback calls `self.model.getMatrixType(id)`).
    #[must_use]
    pub fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        self.inner.matrix_type(matrix_id)
    }
    /// Model's `assemble_matrix`.
    pub fn assemble_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.inner.assemble_matrix(self.dofs, matrix_id)
    }
    /// Model's `assemble_lumped_matrix`.
    pub fn assemble_lumped_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.inner.assemble_lumped_matrix(self.dofs, matrix_id)
    }
    /// Model's `assemble_residual`.
    pub fn assemble_residual(&mut self) -> Result<()> {
        self.inner.assemble_residual(self.dofs)
    }
    /// Model's `predictor`.
    pub fn predictor(&mut self) -> Result<()> {
        self.inner.predictor(self.dofs)
    }
    /// Model's `corrector`.
    pub fn corrector(&mut self) -> Result<()> {
        self.inner.corrector(self.dofs)
    }
    /// Model's `before_solve_step`.
    pub fn before_solve_step(&mut self) -> Result<()> {
        self.inner.before_solve_step(self.dofs)
    }
    /// Model's `after_solve_step`.
    pub fn after_solve_step(&mut self, converged: bool) -> Result<()> {
        self.inner.after_solve_step(self.dofs, converged)
    }
}

/// A user callback intercepting the model callback (upstream
/// `InterceptSolverCallback` subclassed in Python). Every method defaults
/// to delegating, like upstream.
pub trait InterceptCallback {
    /// Matrix type (delegates by default).
    fn matrix_type(&self, model: &dyn SolverCallback, matrix_id: &str) -> MatrixType {
        model.matrix_type(matrix_id)
    }
    /// Assemble a matrix.
    fn assemble_matrix(&mut self, model: &mut Delegate<'_>, matrix_id: &str) -> Result<()> {
        model.assemble_matrix(matrix_id)
    }
    /// Assemble a lumped matrix.
    fn assemble_lumped_matrix(&mut self, model: &mut Delegate<'_>, matrix_id: &str) -> Result<()> {
        model.assemble_lumped_matrix(matrix_id)
    }
    /// Assemble the residual.
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.assemble_residual()
    }
    /// Predictor.
    fn predictor(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.predictor()
    }
    /// Corrector.
    fn corrector(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.corrector()
    }
    /// Before a step.
    fn before_solve_step(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.before_solve_step()
    }
    /// After a step.
    fn after_solve_step(&mut self, model: &mut Delegate<'_>, converged: bool) -> Result<()> {
        model.after_solve_step(converged)
    }
}

/// Adapter combining a user [`InterceptCallback`] and the model callback
/// into one [`SolverCallback`] for the solvers.
pub struct Intercept<'a> {
    user: &'a mut dyn InterceptCallback,
    inner: &'a mut dyn SolverCallback,
}

impl std::fmt::Debug for Intercept<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Intercept").finish_non_exhaustive()
    }
}

impl<'a> Intercept<'a> {
    /// Combine `user` and the model callback `inner`.
    pub fn new(user: &'a mut dyn InterceptCallback, inner: &'a mut dyn SolverCallback) -> Self {
        Self { user, inner }
    }
}

impl SolverCallback for Intercept<'_> {
    fn matrix_type(&self, matrix_id: &str) -> MatrixType {
        self.user.matrix_type(&*self.inner, matrix_id)
    }
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        self.user.assemble_matrix(
            &mut Delegate {
                inner: &mut *self.inner,
                dofs,
            },
            matrix_id,
        )
    }
    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        self.user.assemble_lumped_matrix(
            &mut Delegate {
                inner: &mut *self.inner,
                dofs,
            },
            matrix_id,
        )
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.user.assemble_residual(&mut Delegate {
            inner: &mut *self.inner,
            dofs,
        })
    }
    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        self.inner.assemble_residual_part(dofs, part)
    }
    fn can_split_residual(&self) -> bool {
        self.inner.can_split_residual()
    }
    fn predictor(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.user.predictor(&mut Delegate {
            inner: &mut *self.inner,
            dofs,
        })
    }
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.user.corrector(&mut Delegate {
            inner: &mut *self.inner,
            dofs,
        })
    }
    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.user.before_solve_step(&mut Delegate {
            inner: &mut *self.inner,
            dofs,
        })
    }
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        self.user.after_solve_step(
            &mut Delegate {
                inner: &mut *self.inner,
                dofs,
            },
            converged,
        )
    }
    fn restore_last_converged_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.inner.restore_last_converged_step(dofs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_comm::Communicator;

    struct Model(u32);
    impl SolverCallback for Model {
        fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
            Some(self)
        }
    }
    struct Opaque;
    impl SolverCallback for Opaque {}

    #[test]
    fn split_as_downcasts_only_the_exposed_type() {
        let mut dofs = DofManager::new("dm", Communicator::serial());
        let mut model = Model(7);
        let mut d = Delegate {
            inner: &mut model,
            dofs: &mut dofs,
        };
        assert!(d.split_as::<Opaque>().is_none());
        assert_eq!(d.split_as_ref::<Model>().map(|(m, _)| m.0), Some(7));
        if let Some((m, _)) = d.split_as::<Model>() {
            m.0 = 8;
        }
        assert_eq!(model.0, 8);
        let mut opaque = Opaque;
        let mut d = Delegate {
            inner: &mut opaque,
            dofs: &mut dofs,
        };
        assert!(
            d.split_as::<Opaque>().is_none(),
            "not exposed: as_any_mut is None"
        );
        assert!(d.split_as_ref::<Opaque>().is_none());
    }
}
