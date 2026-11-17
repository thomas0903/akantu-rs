// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/time_step_solvers/
// {time_step_solver.hh, time_step_solver.cc, time_step_solver_default.hh,
// time_step_solver_default.cc}.

//! Time-step solvers (TS-009, TS-010; TS-012 `TimeStepSolverDefaultExplicit`
//! is a declared-only upstream class, approved exception D-028).
//!
//! A [`TimeStepSolver`] owns its non-linear solver and one integration
//! scheme per DOF family. [`TimeStepSolver::solve_step`] drives the
//! non-linear solver with an internal adapter that is itself a
//! [`SolverCallback`] — upstream's `TimeStepSolverDefault` acting as the
//! callback of its non-linear solver — inserting the integration-scheme
//! predictor, corrector, Jacobian and residual contributions around the
//! model callback, in upstream order:
//!
//! * `predictor`: model predictor, then per family (lexicographic):
//!   `savePreviousDOFs` if a previous array exists, scheme predictor;
//! * `corrector`: per family: scheme corrector, increment (`solution` for
//!   a displacement-type solution, else `u − u_previous`; skipped with a
//!   warning without a previous array), then the model corrector;
//! * `assemble_matrix(id ≠ "J")`: if the needed-matrix type of `id` is
//!   defined, create the matrix if missing and call the model;
//!   `assemble_matrix("J")`: create `J` if missing (derived from the needed
//!   matrices, common type), the model assembles every needed matrix with
//!   a defined type (`C`, `K`, `M` order), each scheme assembles `J`, then
//!   `applyBoundary("J")`;
//! * `assemble_residual`: `M` (lumped or consistent) if needed, zero
//!   residual, model residual, scheme residuals (`assemble_residual_part`:
//!   the model part unless `"inertial"`, the scheme residuals only for
//!   `"inertial"`);
//! * `before_solve_step`: model hook, then every scheme stores its state;
//!   `after_solve_step(false)`: schemes restore, then the model hook.
//!
//! Matrices assembled by the model are finalised (collective symmetry
//! agreement) right after their assembly, and `J` after the
//! schemes built it; inside a running collective operation every model
//! callback is a [`DofManager::step`] (its outcome agreed, ARCHITECTURE.md, "Non-linear solvers, time stepping and switches").
//!
//! # Deviations from upstream
//!
//! * **UD-193** (D-003 B): every time-step solver of a DOF manager
//!   shares the matrix `"J"`, but upstream's integration schemes decide
//!   whether to rebuild it from the releases of their own source matrices
//!   only, so after a switch between solvers (static pre-load, then
//!   dynamic steps) each reuses the other's Jacobian (oracle: 10 iterations
//!   without convergence, divergence to 8e18). The port records the value
//!   release of `J` after its own last assembly and makes its schemes
//!   rebuild `J` whenever it changed since;
//!   [`crate::UpstreamCompat::stale_shared_jacobian`] reproduces upstream.
//! * **UD-010**: when the non-linear solver completes a step that
//!   converged at the initial residual test (no solve, no corrector), the
//!   adapter refreshes the increment of every family to `u − u_previous`
//!   before the model's `after_solve_step` (nothing is corrected).
//! * **Resized arrays**: family arrays resized through the public API
//!   ([`DofManager::family_mut`]) are reported as
//!   [`akantu_core::Error::DimensionMismatch`], never a panic or a silently
//!   truncated update.

use std::collections::BTreeMap;

use akantu_core::enums::{IntegrationSchemeType, MatrixType, TimeStepSolverType};
use akantu_core::exec::{Tuples, TuplesRef};
use akantu_core::{Error, Id, Real, Result};

use crate::callback::SolverCallback;
use crate::dof_manager::DofManager;
use crate::integration_scheme::{IntegrationScheme, SolutionType, new_integration_scheme};
use crate::non_linear::NonLinearSolver;

/// One time-step solver (upstream `TimeStepSolverDefault`).
#[derive(Debug)]
pub struct TimeStepSolver {
    /// Identifier.
    pub id: Id,
    /// Kind (`static`, `dynamic`, `dynamic_lumped`).
    pub kind: TimeStepSolverType,
    /// Time step.
    pub time_step: Real,
    /// Non-linear solver.
    pub non_linear_solver: Box<dyn NonLinearSolver>,
    /// Integration scheme per DOF family (lexicographic order, upstream
    /// `std::map`).
    schemes: BTreeMap<Id, Box<dyn IntegrationScheme>>,
    /// Solution type per DOF family.
    solution_types: BTreeMap<Id, SolutionType>,
    /// Needed matrices and their types (upstream `std::map<std::string,
    /// MatrixType>`: lexicographic, `C` < `K` < `M`).
    needed_matrices: BTreeMap<String, MatrixType>,
    is_mass_lumped: bool,
    /// Value release of `"J"` right after this solver's last assembly of
    /// it (UD-193: another solver sharing the manager changed `J` when the
    /// release differs).
    jacobian_release: Option<u64>,
}

impl TimeStepSolver {
    /// New solver of kind `kind` around `non_linear_solver`, time step 0
    /// (upstream constructor; `dynamic_lumped` selects the lumped mass,
    /// `not_defined` is `AKANTU_TO_IMPLEMENT`).
    pub fn new(
        id: impl Into<Id>,
        kind: TimeStepSolverType,
        non_linear_solver: Box<dyn NonLinearSolver>,
    ) -> Result<Self> {
        let is_mass_lumped = match kind {
            TimeStepSolverType::Dynamic | TimeStepSolverType::Static => false,
            TimeStepSolverType::DynamicLumped => true,
            TimeStepSolverType::NotDefined => {
                return Err(Error::NotImplemented(
                    "TimeStepSolverDefault for time-step solver type not_defined",
                ));
            }
        };
        Ok(Self {
            id: id.into(),
            kind,
            time_step: 0.0,
            non_linear_solver,
            schemes: BTreeMap::new(),
            solution_types: BTreeMap::new(),
            needed_matrices: BTreeMap::new(),
            is_mass_lumped,
            jacobian_release: None,
        })
    }

    /// `true` for `dynamic_lumped`.
    #[must_use]
    pub fn is_mass_lumped(&self) -> bool {
        self.is_mass_lumped
    }

    /// Needed matrices and their resolved types.
    #[must_use]
    pub fn needed_matrices(&self) -> &BTreeMap<String, MatrixType> {
        &self.needed_matrices
    }

    /// Upstream `setIntegrationScheme(dof_id, type, solution_type)`:
    /// allocate the scheme (lumped restriction applies), register it and
    /// create the needed matrices whose type the model defines.
    pub fn set_integration_scheme(
        &mut self,
        dofs: &mut DofManager,
        dof_id: &str,
        ty: IntegrationSchemeType,
        solution_type: SolutionType,
        model: &dyn SolverCallback,
    ) -> Result<()> {
        let scheme = new_integration_scheme(ty, dof_id, self.is_mass_lumped)?;
        self.set_integration_scheme_instance(dofs, dof_id, scheme, solution_type, model)
    }

    /// Upstream `setIntegrationScheme(dof_id, scheme, solution_type)` with
    /// an externally built scheme.
    ///
    /// One [`DofManager::collective`] operation: the
    /// duplicate check and the model's matrix types are agreed, then the
    /// needed matrices with a defined type are created (agreed); the scheme
    /// is registered only after every rank succeeded.
    pub fn set_integration_scheme_instance(
        &mut self,
        dofs: &mut DofManager,
        dof_id: &str,
        scheme: Box<dyn IntegrationScheme>,
        solution_type: SolutionType,
        model: &dyn SolverCallback,
    ) -> Result<()> {
        let name = format!("set_integration_scheme:{}:{dof_id}", self.id);
        let needed = dofs.collective(&name, |dofs| {
            let needed = dofs.step("matrix_types", |_| {
                if self.schemes.contains_key(dof_id) {
                    return Err(Error::AlreadyExists {
                        kind: "integration scheme",
                        id: format!(
                            "Their DOFs {dof_id} have already an integration scheme associated"
                        ),
                    });
                }
                let mut needed = self.needed_matrices.clone();
                for name in scheme.needed_matrices() {
                    needed
                        .entry((*name).to_owned())
                        .or_insert(MatrixType::NotDefined);
                }
                for (name, ty) in &mut needed {
                    if *ty == MatrixType::NotDefined {
                        *ty = model.matrix_type(name);
                    }
                }
                Ok(needed)
            })?;
            dofs.step("new_matrices", |d| {
                for (name, ty) in &needed {
                    if *ty != MatrixType::NotDefined && !d.has_matrix(name) {
                        d.new_matrix(name, *ty)?;
                    }
                }
                Ok(())
            })?;
            Ok(needed)
        })?;
        self.needed_matrices = needed;
        self.schemes.insert(dof_id.into(), scheme);
        self.solution_types.insert(dof_id.into(), solution_type);
        Ok(())
    }

    /// Upstream `hasIntegrationScheme`.
    #[must_use]
    pub fn has_integration_scheme(&self, dof_id: &str) -> bool {
        self.schemes.contains_key(dof_id)
    }

    /// Scheme of a family (upstream `getIntegrationScheme`).
    pub fn integration_scheme(&self, dof_id: &str) -> Result<&dyn IntegrationScheme> {
        self.schemes
            .get(dof_id)
            .map(AsRef::as_ref)
            .ok_or_else(|| Error::not_found("integration scheme", dof_id))
    }

    /// Mutable scheme of a family (parameters such as `alpha`, `beta`).
    pub fn integration_scheme_mut(&mut self, dof_id: &str) -> Result<&mut dyn IntegrationScheme> {
        match self.schemes.get_mut(dof_id) {
            Some(s) => Ok(s.as_mut()),
            None => Err(Error::not_found("integration scheme", dof_id)),
        }
    }

    /// Solution type registered for a family.
    #[must_use]
    pub fn solution_type(&self, dof_id: &str) -> Option<SolutionType> {
        self.solution_types.get(dof_id).copied()
    }

    /// Advance one step (upstream `TimeStepSolverDefault::solveStep`): the
    /// non-linear solver drives the adapter around `callback`. A solver
    /// without any integration scheme (a port extension: upstream static
    /// solvers always have `pseudo_time`, and the adapter would then skip
    /// every matrix) lets the non-linear solver drive `callback` directly.
    pub fn solve_step(
        &mut self,
        dofs: &mut DofManager,
        callback: &mut dyn SolverCallback,
    ) -> Result<()> {
        if self.schemes.is_empty() {
            return self.non_linear_solver.solve(dofs, callback);
        }
        let Self {
            non_linear_solver,
            schemes,
            solution_types,
            needed_matrices,
            is_mass_lumped,
            time_step,
            jacobian_release,
            ..
        } = self;
        let mut adapter = Adapter {
            schemes,
            solution_types,
            needed: needed_matrices,
            is_mass_lumped: *is_mass_lumped,
            time_step: *time_step,
            model: callback,
            jacobian_release,
            corrected: false,
        };
        non_linear_solver.solve(dofs, &mut adapter)
    }
}

/// The time-step solver as the callback of its non-linear solver.
struct Adapter<'a> {
    schemes: &'a mut BTreeMap<Id, Box<dyn IntegrationScheme>>,
    solution_types: &'a BTreeMap<Id, SolutionType>,
    needed: &'a mut BTreeMap<String, MatrixType>,
    is_mass_lumped: bool,
    time_step: Real,
    model: &'a mut dyn SolverCallback,
    /// The owning solver's `J` release after its last assembly (UD-193).
    jacobian_release: &'a mut Option<u64>,
    /// The corrector ran during this step (a step completed without
    /// it converged at the initial residual test).
    corrected: bool,
}

fn check_len(context: &'static str, got: usize, expected: usize) -> Result<()> {
    if got == expected {
        Ok(())
    } else {
        Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        })
    }
}

/// Upstream increment update of `TimeStepSolverDefault::corrector`:
/// `increment = solution` (`from_solution`, displacement-type solutions)
/// or `u − u_previous`; skipped with upstream's warning without a previous
/// array. Array lengths are validated.
/// `increment = u − previous` for one part of the equations (no-alias
/// parameters).
#[inline(never)]
fn increment_part(inc: &mut [Real], u: &[Real], prev: &[Real]) {
    for ((i, &x), &p) in inc.iter_mut().zip(u).zip(prev) {
        *i = x - p;
    }
}

fn update_increment(dofs: &mut DofManager, dof_id: &str, from_solution: bool) -> Result<()> {
    if !dofs.has_dofs_increment(dof_id)? {
        return Ok(());
    }
    if !dofs.has_previous_dofs(dof_id)? {
        // upstream AKANTU_DEBUG_WARNING (time_step_solver_default.cc:194)
        akantu_core::aka_debug_warning!(
            "In order to compute the increment of {dof_id} a 'previous' has to be registered"
        );
        return Ok(());
    }
    let exec = dofs.exec().clone();
    let crate::dof_manager::DofData {
        dofs: u,
        increment,
        previous,
        solution,
        ..
    } = dofs.family_mut(dof_id)?;
    let (Some(inc), Some(prev)) = (increment.as_mut(), previous.as_ref()) else {
        return Ok(());
    };
    let (mut inc, prev, u) = (inc.write()?, prev.read()?, u.read()?);
    let n = u.as_slice().len();
    check_len("increment of a DOF family", inc.as_slice().len(), n)?;
    // every equation from its own values: parts on the DOF manager's
    // threads (D-123)
    let part = crate::integration_scheme::EQUATIONS_PER_PART;
    if from_solution {
        check_len("solution of a DOF family", solution.as_slice().len(), n)?;
        let items = (
            Tuples::new(inc.as_mut_slice(), 1),
            TuplesRef::new(solution.as_slice(), 1),
        );
        exec.for_each(items, part, |_, (mut i, s)| {
            i.as_mut_slice().copy_from_slice(s.as_slice());
        });
    } else {
        check_len("previous values of a DOF family", prev.as_slice().len(), n)?;
        let items = (
            Tuples::new(inc.as_mut_slice(), 1),
            TuplesRef::new(u.as_slice(), 1),
            TuplesRef::new(prev.as_slice(), 1),
        );
        exec.for_each(items, part, |_, (mut i, x, p)| {
            increment_part(i.as_mut_slice(), x.as_slice(), p.as_slice());
        });
    }
    Ok(())
}

/// One model callback: an agreed step inside a running collective
/// operation, a plain call otherwise (callers driving the adapter outside
/// the collective protocol).
fn model_step<R>(
    dofs: &mut DofManager,
    name: &str,
    f: impl FnOnce(&mut DofManager) -> Result<R>,
) -> Result<R> {
    if dofs.in_collective() {
        dofs.step(name, f)
    } else {
        f(dofs)
    }
}

impl Adapter<'_> {
    fn solution_type(&self, dof_id: &str) -> SolutionType {
        self.solution_types
            .get(dof_id)
            .copied()
            .unwrap_or(SolutionType::NotDefined)
    }

    /// Upstream `getCommonMatrixType` (`min` over the needed types).
    fn common_type(&self) -> MatrixType {
        self.needed
            .values()
            .fold(MatrixType::NotDefined, |a, &b| a.min(b))
    }

    /// Upstream `TimeStepSolver::assembleMatrix`.
    fn assemble_matrix_base(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        let common = self.common_type();
        if matrix_id != "J" {
            let ty = *self
                .needed
                .entry(matrix_id.to_owned())
                .or_insert(MatrixType::NotDefined);
            if ty == MatrixType::NotDefined {
                return Ok(());
            }
            if !dofs.has_matrix(matrix_id) {
                dofs.new_matrix(matrix_id, ty)?;
            }
            let model = &mut *self.model;
            model_step(dofs, &format!("model:assemble_matrix:{matrix_id}"), |d| {
                model.assemble_matrix(d, matrix_id)
            })?;
            return dofs.finalize_matrix(matrix_id);
        }
        if !dofs.has_matrix("J") {
            if common == MatrixType::NotDefined {
                return Err(Error::invalid("No type defined for the matrices"));
            }
            let sources: Vec<&str> = self
                .needed
                .iter()
                .filter(|(n, t)| **t != MatrixType::NotDefined && dofs.has_matrix(n))
                .map(|(n, _)| n.as_str())
                .collect();
            if sources.is_empty() {
                dofs.new_matrix("J", common)?;
            } else {
                dofs.new_derived_matrix("J", &sources)?;
            }
        }
        let names: Vec<String> = self
            .needed
            .iter()
            .filter(|(_, t)| **t != MatrixType::NotDefined)
            .map(|(n, _)| n.clone())
            .collect();
        for name in names {
            let model = &mut *self.model;
            model_step(dofs, &format!("model:assemble_matrix:{name}"), |d| {
                model.assemble_matrix(d, &name)
            })?;
            dofs.finalize_matrix(&name)?;
        }
        Ok(())
    }

    fn assemble_mass(&mut self, dofs: &mut DofManager) -> Result<()> {
        if self.needed.contains_key("M") {
            if self.is_mass_lumped {
                self.assemble_lumped_matrix(dofs, "M")?;
            } else {
                self.assemble_matrix(dofs, "M")?;
            }
        }
        Ok(())
    }
}

impl SolverCallback for Adapter<'_> {
    fn matrix_type(&self, _: &str) -> MatrixType {
        MatrixType::NotDefined
    }

    fn predictor(&mut self, dofs: &mut DofManager) -> Result<()> {
        let model = &mut *self.model;
        model_step(dofs, "model:predictor", |d| model.predictor(d))?;
        let dt = self.time_step;
        for (dof_id, scheme) in self.schemes.iter_mut() {
            if dofs.has_previous_dofs(dof_id)? {
                dofs.save_previous_dofs(dof_id)?;
            }
            scheme.predictor(dofs, dt)?;
        }
        Ok(())
    }

    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.corrected = true;
        let dt = self.time_step;
        for (dof_id, scheme) in self.schemes.iter_mut() {
            let st = self
                .solution_types
                .get(dof_id)
                .copied()
                .unwrap_or(SolutionType::NotDefined);
            scheme.corrector(dofs, st, dt)?;
            // `u − previous` is computed from the corrected values as
            // upstream (`u` first, then the subtraction, per component).
            update_increment(dofs, dof_id, st == SolutionType::Displacement)?;
        }
        let model = &mut *self.model;
        model_step(dofs, "model:corrector", |d| model.corrector(d))
    }

    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        self.assemble_matrix_base(dofs, matrix_id)?;
        if matrix_id != "J" {
            return Ok(());
        }
        // UD-193: `J` changed since this solver last assembled it (another
        // time-step solver of the manager rebuilt it): the schemes' release
        // caches no longer describe its content.
        let track = !dofs.upstream_compat().stale_shared_jacobian;
        if track {
            let current = dofs.matrix_release("J").ok();
            if current.is_none() || current != *self.jacobian_release {
                for scheme in self.schemes.values_mut() {
                    scheme.invalidate_jacobian();
                }
            }
        }
        let dt = self.time_step;
        let ids: Vec<Id> = self.schemes.keys().cloned().collect();
        for dof_id in ids {
            let st = self.solution_type(&dof_id);
            if let Some(scheme) = self.schemes.get_mut(&dof_id) {
                scheme.assemble_jacobian(dofs, st, dt)?;
            }
        }
        dofs.finalize_matrix("J")?;
        dofs.apply_boundary("J")?;
        if track {
            *self.jacobian_release = dofs.matrix_release("J").ok();
        }
        Ok(())
    }

    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if !dofs.has_lumped_matrix(matrix_id) {
            dofs.new_lumped_matrix(matrix_id)?;
        }
        let model = &mut *self.model;
        model_step(
            dofs,
            &format!("model:assemble_lumped_matrix:{matrix_id}"),
            |d| model.assemble_lumped_matrix(d, matrix_id),
        )
    }

    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.assemble_mass(dofs)?;
        dofs.zero_residual();
        let model = &mut *self.model;
        model_step(dofs, "model:assemble_residual", |d| {
            model.assemble_residual(d)
        })?;
        let lumped = self.is_mass_lumped;
        for scheme in self.schemes.values_mut() {
            scheme.assemble_residual(dofs, lumped)?;
        }
        Ok(())
    }

    fn assemble_residual_part(&mut self, dofs: &mut DofManager, part: &str) -> Result<()> {
        self.assemble_mass(dofs)?;
        if part != "inertial" {
            let model = &mut *self.model;
            model_step(dofs, &format!("model:assemble_residual:{part}"), |d| {
                model.assemble_residual_part(d, part)
            })?;
        } else {
            let lumped = self.is_mass_lumped;
            for scheme in self.schemes.values_mut() {
                scheme.assemble_residual(dofs, lumped)?;
            }
        }
        Ok(())
    }

    fn can_split_residual(&self) -> bool {
        self.model.can_split_residual()
    }

    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        let model = &mut *self.model;
        model_step(dofs, "model:before_solve_step", |d| {
            model.before_solve_step(d)
        })?;
        for scheme in self.schemes.values_mut() {
            scheme.store(dofs)?;
        }
        Ok(())
    }

    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        if !converged {
            self.restore_last_converged_step(dofs)?;
        } else if !self.corrected {
            // UD-010 fix: the step converged at the
            // initial residual test (nothing solved, no scheme correction);
            // the increment is the step's change `u − u_previous` (0 for
            // pseudo-time), not the previous step's value.
            let ids: Vec<Id> = self.schemes.keys().cloned().collect();
            for dof_id in ids {
                update_increment(dofs, &dof_id, false)?;
            }
        }
        let model = &mut *self.model;
        model_step(dofs, "model:after_solve_step", |d| {
            model.after_solve_step(d, converged)
        })
    }

    /// Upstream `TimeStepSolverDefault::restoreLastConvergedStep`: every
    /// scheme restores its stored state (the model hook is not called).
    fn restore_last_converged_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        for scheme in self.schemes.values_mut() {
            scheme.restore(dofs)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_comm::Communicator;
    use akantu_core::debug::{self, DebugLevel};

    /// The missing-previous warning is upstream's `AKANTU_DEBUG_WARNING`
    /// (`time_step_solver_default.cc:194`): printed by the debugger at
    /// `dblWarning` with prefix `/!\` and the location, filtered below it;
    /// the increment is left alone.
    #[test]
    fn increment_without_previous_warns_through_the_debugger() {
        let mut dofs = DofManager::new("dm", Communicator::serial());
        dofs.register_generic_dofs("lambda", 2, 1).unwrap();
        dofs.register_dofs_increment("lambda").unwrap();
        let (r, text) = debug::capture(DebugLevel::WARNING, || {
            update_increment(&mut dofs, "lambda", false)
        });
        r.unwrap();
        let line = text.lines().next().unwrap_or_default();
        let (_, msg) = line.split_once("} ").unwrap();
        assert!(
            msg.starts_with(
                "/!\\ In order to compute the increment of lambda a 'previous' has to be \
                 registered (update_increment(): crates/akantu-solver/src/time_step.rs:"
            ),
            "{text}"
        );
        assert_eq!(text.lines().count(), 1, "{text}");
        let (r, text) = debug::capture(DebugLevel::ERROR, || {
            update_increment(&mut dofs, "lambda", false)
        });
        r.unwrap();
        assert!(text.is_empty(), "{text}");
    }
}
