// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_linear_solver/
// {non_linear_solver_lumped.hh, non_linear_solver_lumped.cc}.

//! Lumped (explicit) solver (NLS-002, upstream `NonLinearSolverLumped`).
//!
//! `solve`: `before_solve_step`, blocked snapshot, `predictor`, the
//! callback's residual (never split), `x = b_a2x · (r / M_lumped)` over
//! every local equation (blocked ones included — the correctors skip
//! them), `splitSolutionPerDOFs`, `corrector`, `after_solve_step(true)`.
//! One [`DofManager::collective`] operation with every callback agreed
//! ([`DofManager::step`], `docs/ARCHITECTURE.md`).

use akantu_core::enums::NonLinearSolverType;
use akantu_core::{Error, Id, Real, Result};
use akantu_parser::ParserType;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    Parsable, table_get, table_info, table_names, table_set, table_set_from_input,
};

use crate::callback::SolverCallback;
use crate::dof_manager::DofManager;
use crate::non_linear::NonLinearSolver;
use crate::non_linear_newton_raphson::parse_enum;

/// Lumped solver (upstream `NonLinearSolverLumped`).
#[derive(Debug, Clone)]
pub struct NonLinearSolverLumped {
    id: Id,
    non_linear_solver_type: NonLinearSolverType,
    /// `b_a2x`: conversion coefficient between `x` and `A⁻¹ b`.
    alpha: Real,
}

impl NonLinearSolverLumped {
    /// New lumped solver (`ty` must be `lumped` or `auto`, upstream
    /// `checkIfTypeIsSupported`).
    pub fn new(id: &str, ty: NonLinearSolverType) -> Result<Self> {
        if ty != NonLinearSolverType::Lumped && ty != NonLinearSolverType::Auto {
            return Err(Error::invalid(format!(
                "The resolution method {ty} is not implemented in the non linear solver {id}!"
            )));
        }
        Ok(Self {
            id: id.into(),
            non_linear_solver_type: ty,
            alpha: 1.0,
        })
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

static LUMPED_PARAMS: &[ParamDescriptor<NonLinearSolverLumped>] = &[
    ParamDescriptor::new(
        "type",
        ParamKind::Enum,
        ParameterAccess::PARSABLE,
        "Non linear solver type",
        |s| ParamValue::Enum(s.non_linear_solver_type.to_string()),
        Some(|s, v| {
            s.non_linear_solver_type = parse_enum(&v)?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "b_a2x",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "Conversion coefficient between x and A^{-1} b",
        |s| ParamValue::Real(s.alpha),
        Some(|s, v| {
            s.alpha = v.as_real()?;
            Ok(())
        }),
    ),
];

impl ParameterRegistry for NonLinearSolverLumped {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(LUMPED_PARAMS, self, name)
    }
    fn set_param(&mut self, name: &str, v: ParamValue) -> Result<()> {
        table_set(LUMPED_PARAMS, self, name, v)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(LUMPED_PARAMS)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(LUMPED_PARAMS, name)
    }
    fn set_param_from_input(&mut self, name: &str, v: ParamValue) -> Result<()> {
        table_set_from_input(LUMPED_PARAMS, self, name, v)
    }
}

impl Parsable for NonLinearSolverLumped {
    fn section_type(&self) -> ParserType {
        ParserType::NonLinearSolver
    }
    fn parsable_id(&self) -> &str {
        &self.id
    }
}

impl NonLinearSolver for NonLinearSolverLumped {
    fn kind(&self) -> NonLinearSolverType {
        self.non_linear_solver_type
    }

    fn solve(&mut self, dofs: &mut DofManager, cb: &mut dyn SolverCallback) -> Result<()> {
        let alpha = self.alpha;
        dofs.collective(&format!("nls:{}", self.id), |dofs| {
            dofs.step("before_solve_step", |d| cb.before_solve_step(d))?;
            dofs.update_global_blocked_dofs();
            dofs.step("predictor", |d| cb.predictor(d))?;
            dofs.step("assemble_residual", |d| cb.assemble_residual(d))?;
            dofs.step("solve_lumped", |d| d.solve_lumped("M", alpha))?;
            dofs.step("corrector", |d| cb.corrector(d))?;
            dofs.step("after_solve_step", |d| cb.after_solve_step(d, true))
        })
    }

    /// Not an upstream quantity of the lumped solver: one direct solve.
    fn nb_iterations(&self) -> usize {
        1
    }

    /// Not an upstream quantity of the lumped solver.
    fn error(&self) -> f64 {
        0.0
    }

    /// The lumped solve always succeeds once it ran.
    fn converged(&self) -> bool {
        true
    }
}
