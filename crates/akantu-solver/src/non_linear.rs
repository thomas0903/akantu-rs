// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_linear_solver/
// {non_linear_solver.hh, non_linear_solver_newton_raphson.*,
// non_linear_solver_lumped.*, non_linear_solver_linear (in newton_raphson),
// non_linear_solver_petsc.*, non_linear_solver_tao.*}.

//! Non-linear solvers (NLS-001, NLS-009): the trait, the factory of the
//! default DOF manager (upstream `DOFManagerDefault::getNewNonLinearSolver`)
//! and the native-backend driver. Implementations:
//! [`crate::NonLinearSolverNewtonRaphson`] (Newton–Raphson family and
//! `linear`), [`crate::NonLinearSolverLumped`].
//!
//! Parameters (`max_iterations`, `threshold`, `convergence_type`, …) are
//! registered with their upstream names so `.dat` `non_linear_solver`
//! sections and Python `getNonLinearSolver().set(...)` work unchanged.
//! Convergence tests reproduce upstream exactly (same norm, same
//! comparison `<`, same iteration counting), since iteration counts are
//! compared in differential tests.

use akantu_core::Result;
use akantu_core::enums::{NonLinearSolverType, SparseSolverType};
use akantu_parser::ParameterRegistry;

use crate::callback::SolverCallback;
use crate::dof_manager::DofManager;
use crate::non_linear_lumped::NonLinearSolverLumped;
use crate::non_linear_newton_raphson::NonLinearSolverNewtonRaphson;

/// Non-linear solver of the default (AIJ) DOF manager for `ty` (upstream
/// `DOFManagerDefault::getNewNonLinearSolver`): the Newton–Raphson types
/// share one implementation, `linear` is the single-solve variant,
/// `lumped` the explicit solver; PETSc types (and `auto`, resolved by the
/// model solver first) are not supported by this DOF manager. `sparse` is
/// the sparse direct solver of the Newton–Raphson family (D-040; `Auto`
/// resolves like upstream).
pub fn new_non_linear_solver(
    dofs: &mut DofManager,
    id: &str,
    ty: NonLinearSolverType,
    sparse: SparseSolverType,
) -> Result<Box<dyn NonLinearSolver>> {
    use NonLinearSolverType as T;
    if dofs.backend().kind() == crate::backend::BackendKind::Petsc {
        return new_non_linear_solver_petsc(dofs, id, ty, sparse);
    }
    Ok(match ty {
        T::NewtonRaphson | T::NewtonRaphsonContact | T::NewtonRaphsonModified => Box::new(
            NonLinearSolverNewtonRaphson::new(dofs, id, ty, sparse, false)?,
        ),
        T::Linear => Box::new(NonLinearSolverNewtonRaphson::new(
            dofs, id, ty, sparse, true,
        )?),
        T::Lumped => Box::new(NonLinearSolverLumped::new(id, ty)?),
        T::PetscSnes | T::PetscTao | T::Auto => {
            return Err(akantu_core::Error::invalid(format!(
                "The asked type of non linear solver ({ty}) is not supported by this dof manager"
            )));
        }
    })
}

/// Non-linear solver of the PETSc DOF manager for `ty` (upstream
/// `DOFManagerPETSc::getNewNonLinearSolver`): `petsc_snes` (also the
/// fallback of unknown kinds upstream, here `auto`) and `petsc_tao` are
/// native solvers, which require the `petsc` sparse solver (upstream
/// exception "petsc non linear solver works only with petsc sparse
/// solver"); the Newton–Raphson family, `linear` and `lumped` use the
/// generic implementations, the Newton–Raphson ones solving with the
/// backend's KSP.
fn new_non_linear_solver_petsc(
    dofs: &mut DofManager,
    id: &str,
    ty: NonLinearSolverType,
    sparse: SparseSolverType,
) -> Result<Box<dyn NonLinearSolver>> {
    use NonLinearSolverType as T;
    let native_needs_petsc = || {
        if sparse == SparseSolverType::Petsc {
            Ok(())
        } else {
            Err(akantu_core::Error::invalid(
                "petsc non linear solver works only with petsc sparse solver",
            ))
        }
    };
    Ok(match ty {
        T::PetscTao => {
            native_needs_petsc()?;
            Box::new(NativeNonLinear::petsc_tao(id))
        }
        T::NewtonRaphson | T::NewtonRaphsonContact | T::NewtonRaphsonModified => Box::new(
            NonLinearSolverNewtonRaphson::new(dofs, id, ty, sparse, false)?,
        ),
        T::Linear => Box::new(NonLinearSolverNewtonRaphson::new(
            dofs, id, ty, sparse, true,
        )?),
        T::Lumped => Box::new(NonLinearSolverLumped::new(id, ty)?),
        T::PetscSnes | T::Auto => {
            native_needs_petsc()?;
            Box::new(NativeNonLinear::petsc_snes(id))
        }
    })
}

/// A non-linear solver (upstream `NonLinearSolver`).
pub trait NonLinearSolver: ParameterRegistry + std::fmt::Debug + Send {
    /// Kind.
    fn kind(&self) -> NonLinearSolverType;

    /// Solve the non-linear system defined by `callback` (upstream
    /// `solve(SolverCallback &)`). Returns `Error::NotConverged` when the
    /// iteration limit is reached (upstream throws
    /// `NLSNotConvergedException`).
    fn solve(&mut self, dofs: &mut DofManager, callback: &mut dyn SolverCallback) -> Result<()>;

    /// Iterations of the last solve.
    fn nb_iterations(&self) -> usize;

    /// Final error of the last solve.
    fn error(&self) -> f64;

    /// Whether the last solve converged.
    fn converged(&self) -> bool;

    /// The Newton–Raphson solver behind this object, if it is one (UD-010
    /// legacy hook switch, sparse-solver inspection).
    fn as_newton_raphson_mut(&mut self) -> Option<&mut NonLinearSolverNewtonRaphson> {
        None
    }

    /// The native (PETSc SNES/TAO) solver behind this object, if it is one
    /// (bounds, options, diagnostics).
    fn as_native_mut(&mut self) -> Option<&mut NativeNonLinear> {
        None
    }

    /// Parse a `non_linear_solver` section with solver-specific rules
    /// (upstream `ParsablePETSc::parseSection`: every parameter becomes a
    /// PETSc option); `None` uses the generic parameter parsing.
    fn parse_options_section(
        &mut self,
        section: &akantu_parser::ParserSection,
    ) -> Option<Result<()>> {
        let _ = section;
        None
    }
}

/// Non-linear solver whose iterations run **inside the linear-system
/// backend** (upstream `NonLinearSolverPETSc` / `NonLinearSolverTAO`): the
/// backend's [`crate::NativeNonLinearDriver`] calls back into the model and
/// the DOF manager through a [`crate::NativeSolveContext`] and solves its
/// linear systems with the backend-owned matrix. Lifecycle
/// as upstream `NonLinearSolverPETSc::solve`: `before_solve_step`,
/// `predictor`, (PETSc kinds: `assemble_matrix("J")` — before the
/// predictor for TAO, after it for SNES, as upstream), native solve,
/// `after_solve_step(converged)`.
///
/// **Collective protocol.** The whole solve is one
/// [`DofManager::collective`] operation: each lifecycle callback, the
/// acquisition of the native driver and the driver's outcome are agreed on
/// every rank before anything depends on them ([`DofManager::step`]); the
/// trampolines join the same scope and return status codes that are
/// identical on every rank, so the native driver's control flow cannot
/// diverge between ranks. After the first failure the scope is failed on
/// every rank: later trampolines return a nonzero status at once without
/// communicating, and the solve returns an error on every rank (the
/// failing rank's own error, `"failed on another rank"` elsewhere).
///
/// **PETSc parameters (NLS-006/007/008, upstream `ParsablePETSc`).** The
/// read-only parameters of the last solve (`n_iter`,
/// `convergence_reason`, `function_norm`, `solution_norm`, `update_norm`,
/// `error` for SNES) are readable; setting any other name — from Python
/// `set` or a `non_linear_solver` section of the input file — stores the
/// PETSc option `-<name> <value>` in the solver's private database, applied
/// at the next solve around `SNESSetFromOptions`/`TaoSetFromOptions`
/// (command-line / `PETSC_OPTIONS` options win). akantu's own names
/// (`max_iterations`, `threshold`, `convergence_type`; for TAO also
/// `absolute_threshold`, `relative_threshold`) are refused with upstream's
/// message naming the PETSc option to use instead.
#[derive(Debug)]
pub struct NativeNonLinear {
    matrix_id: akantu_core::Id,
    kind: Option<NonLinearSolverType>,
    solver_id: akantu_core::Id,
    /// Private options database in setting order: typed values of
    /// `set_param` (formatted at each solve, UD-424) or raw section text.
    options: Vec<(String, StoredOption)>,
    bounds: Option<(Vec<f64>, Vec<f64>)>,
    /// Why the bounds no longer name the current local equations (a
    /// renumbering since they were set).
    stale_bounds: Option<String>,
    report: Option<crate::backend::NativeReport>,
    iterations: usize,
    error: f64,
    converged: bool,
}

/// A stored PETSc option value.
#[derive(Clone, Debug)]
enum StoredOption {
    /// `set_param` value (upstream `ParsablePETSc::set` formats it).
    Value(akantu_parser::param::ParamValue),
    /// Raw parser-section text (upstream `param.getValue()`).
    Raw(String),
}

/// Upstream `akantu_to_petsc_options` of `NonLinearSolverPETSc`.
const SNES_REFUSED: [(&str, &str); 3] = [
    ("max_iterations", "snes_max_it"),
    ("threshold", "snes_atol\", \"snes_rtol\" or \"snes_stol"),
    (
        "convergence_type",
        "snes_atol\", \"snes_rtol\" or \"snes_stol",
    ),
];

/// Upstream `akantu_to_petsc_options` of `NonLinearSolverTAO`.
const TAO_REFUSED: [(&str, &str); 5] = [
    ("max_iterations", "tao_max_it"),
    ("threshold", "tao_gatol\", \"tao_grtol\" or \"tao_gttol"),
    (
        "convergence_type",
        "tao_gatol\", \"tao_grtol\" or \"tao_gttol",
    ),
    ("absolute_threshold", "tao_gatol"),
    ("relative_threshold", "tao_grtol"),
];

/// Parameters `NonLinearSolverTAO` registers (`_pat_parsmod`,
/// `non_linear_solver_tao.cc:68-76`): readable through `get` with their
/// registration values (`max_iterations` 10, `threshold` 1e-10,
/// `convergence_type` `_solution`), never changed — `set` and sections
/// refuse these names first (`exceptIfWrongParameter`) — and unused by the
/// TAO solve.
const TAO_READABLE: [&str; 3] = ["max_iterations", "threshold", "convergence_type"];

/// Read-only parameters of `NonLinearSolverPETSc` (registration order).
const SNES_READABLE: [&str; 6] = [
    "n_iter",
    "convergence_reason",
    "function_norm",
    "solution_norm",
    "update_norm",
    "error",
];

impl NativeNonLinear {
    /// Native solver over the Jacobian `matrix_id` (`"J"` upstream), for
    /// test or third-party backends (no PETSc parameters, no
    /// pre-assembly).
    #[must_use]
    pub fn new(matrix_id: impl Into<akantu_core::Id>) -> Self {
        Self {
            matrix_id: matrix_id.into(),
            kind: None,
            solver_id: akantu_core::Id::default(),
            options: Vec::new(),
            bounds: None,
            stale_bounds: None,
            report: None,
            iterations: 0,
            error: 0.0,
            converged: false,
        }
    }

    /// Upstream `NonLinearSolverPETSc` (`petsc_snes`) with id `solver_id`
    /// over `"J"`.
    #[must_use]
    pub fn petsc_snes(solver_id: impl Into<akantu_core::Id>) -> Self {
        Self {
            kind: Some(NonLinearSolverType::PetscSnes),
            solver_id: solver_id.into(),
            ..Self::new("J")
        }
    }

    /// Upstream `NonLinearSolverTAO` (`petsc_tao`) with id `solver_id` over
    /// `"J"`.
    #[must_use]
    pub fn petsc_tao(solver_id: impl Into<akantu_core::Id>) -> Self {
        Self {
            kind: Some(NonLinearSolverType::PetscTao),
            solver_id: solver_id.into(),
            ..Self::new("J")
        }
    }

    /// Upstream `NonLinearSolverTAO::setBounds`: bounds on the **increment**
    /// of the next solves, over local equations (owned rows are used).
    /// Upstream's TAO objective (reproduced) is exact only when the bounds
    /// contain the increment 0 (UD-1022): a phase-field step uses
    /// `0 ≤ δ ≤ 1 − d`, built with `DofManager::assemble_to_global_array`
    /// as upstream's tests do. A solve whose bounds exclude `δ = 0` on some
    /// owned row (`lower > 0` or `upper < 0`) is refused (D-088: an agreed,
    /// recoverable `Error::Unsupported` naming UD-1022, raised before
    /// anything of the solve runs) unless the DOF manager's
    /// upstream-compatibility switch
    /// `UpstreamCompat::tao_bounds_excluding_zero_increment` is set, which
    /// reproduces upstream's (wrong) result.
    pub fn set_bounds(&mut self, lower: Vec<f64>, upper: Vec<f64>) -> Result<()> {
        if self.kind != Some(NonLinearSolverType::PetscTao) {
            return Err(akantu_core::Error::invalid(
                "variable bounds are only supported by the petsc_tao solver",
            ));
        }
        if lower.len() != upper.len() {
            return Err(akantu_core::Error::DimensionMismatch {
                context: "NativeNonLinear::set_bounds",
                expected: lower.len(),
                got: upper.len(),
            });
        }
        self.bounds = Some((lower, upper));
        self.stale_bounds = None;
        Ok(())
    }

    /// Invalidate the bounds after a renumbering of the local equations:
    /// bounds name local equations, so after a mesh
    /// change that renumbers them (a node removal, SMM-015) the next bounded
    /// solve is refused — an agreed, recoverable error naming `cause` —
    /// until [`NativeNonLinear::set_bounds`] is called again. The bounds are
    /// not moved with the equations (set them again after any mesh change).
    /// [`crate::ModelSolver::on_dof_mesh_event`] calls this for the solvers
    /// of a model; without bounds it does nothing.
    pub fn invalidate_bounds(&mut self, cause: &str) {
        if self.bounds.is_some() {
            self.stale_bounds = Some(cause.to_owned());
        }
    }

    /// The private options database (`-name value`, in setting order),
    /// reals formatted with round-trip digits (or upstream's 6 significant
    /// digits with `upstream_reals`, UD-424).
    #[must_use]
    pub fn options_formatted(&self, upstream_reals: bool) -> Vec<(String, String)> {
        self.options
            .iter()
            .filter_map(|(k, v)| {
                let text = match v {
                    StoredOption::Raw(t) => t.clone(),
                    StoredOption::Value(p) => format_value(p, upstream_reals).ok()?,
                };
                (!text.is_empty()).then(|| (k.clone(), text))
            })
            .collect()
    }

    /// [`NativeNonLinear::options_formatted`] with the default formatting.
    #[must_use]
    pub fn options(&self) -> Vec<(String, String)> {
        self.options_formatted(false)
    }

    /// Diagnostics of the last solve.
    #[must_use]
    pub fn report(&self) -> Option<&crate::backend::NativeReport> {
        self.report.as_ref()
    }

    fn refused(&self) -> &'static [(&'static str, &'static str)] {
        match self.kind {
            Some(NonLinearSolverType::PetscSnes) => &SNES_REFUSED,
            Some(NonLinearSolverType::PetscTao) => &TAO_REFUSED,
            _ => &[],
        }
    }

    fn readable(&self) -> &'static [&'static str] {
        match self.kind {
            Some(NonLinearSolverType::PetscSnes) => &SNES_READABLE,
            Some(NonLinearSolverType::PetscTao) => &TAO_READABLE,
            _ => &[],
        }
    }

    /// Upstream `ParsablePETSc::exceptIfWrongParameter`.
    fn except_if_wrong_parameter(&self, name: &str) -> Result<()> {
        if let Some((_, petsc)) = self.refused().iter().find(|(n, _)| *n == name) {
            let (ty, obj) = match self.kind {
                Some(NonLinearSolverType::PetscTao) => ("gpcg", "tao"),
                _ => ("newtonls", "snes"),
            };
            return Err(akantu_core::Error::invalid(format!(
                "Cannot set the parameter \"{name}\" on object \"{}\" of type \"{ty}\" use the \
                 PETSc option \"{petsc}\" instead. ({obj})",
                self.solver_id
            )));
        }
        Ok(())
    }
}

/// Upstream `ParsablePETSc::set` value conversion: reals through
/// `std::stringstream` (6 significant digits, UD-424) or round-trip digits,
/// integers, `true`/`false`, strings as given.
fn format_value(value: &akantu_parser::param::ParamValue, upstream_reals: bool) -> Result<String> {
    use akantu_parser::param::ParamValue as V;
    Ok(match value {
        V::Real(r) => {
            if upstream_reals {
                akantu_linalg::matrix_market::format_g(*r, 6)
            } else {
                format!("{r:?}")
            }
        }
        V::Int(i) => i.to_string(),
        V::Bool(b) => if *b { "true" } else { "false" }.to_owned(),
        V::String(t) | V::Enum(t) => t.clone(),
        V::ByteString(b) => b.to_string_lossy().into_owned(),
        other => {
            return Err(akantu_core::Error::invalid(format!(
                "PETSc options take a real, an integer, a boolean or a string, not {:?}",
                other.kind()
            )));
        }
    })
}

impl ParameterRegistry for NativeNonLinear {
    fn get_param(&self, name: &str) -> Result<akantu_parser::param::ParamValue> {
        use akantu_parser::param::ParamValue as V;
        let r = self.report.clone().unwrap_or_default();
        match (self.kind, name) {
            (Some(NonLinearSolverType::PetscSnes), "n_iter") => {
                Ok(V::Int(i64::try_from(self.iterations).unwrap_or(i64::MAX)))
            }
            (Some(NonLinearSolverType::PetscSnes), "convergence_reason") => {
                Ok(V::String(r.reason_string))
            }
            (Some(NonLinearSolverType::PetscSnes), "function_norm") => Ok(V::Real(r.function_norm)),
            (Some(NonLinearSolverType::PetscSnes), "solution_norm") => Ok(V::Real(r.solution_norm)),
            (Some(NonLinearSolverType::PetscSnes), "update_norm") => Ok(V::Real(r.update_norm)),
            (Some(NonLinearSolverType::PetscSnes), "error") => Ok(V::Real(self.error)),
            (Some(NonLinearSolverType::PetscTao), "max_iterations") => Ok(V::Int(10)),
            (Some(NonLinearSolverType::PetscTao), "threshold") => Ok(V::Real(1e-10)),
            (Some(NonLinearSolverType::PetscTao), "convergence_type") => Ok(V::Enum(
                akantu_core::enums::SolveConvergenceCriteria::Solution.to_string(),
            )),
            _ => Err(akantu_core::Error::not_found("parameter", name)),
        }
    }

    /// Upstream `ParsablePETSc::set`: refused akantu names, read-only
    /// parameters, otherwise a PETSc option of the private database.
    fn set_param(&mut self, name: &str, value: akantu_parser::param::ParamValue) -> Result<()> {
        if self.kind.is_none() {
            // Native options go through the backend's options database
            // (PETSc prefixes, INT-001).
            return Err(akantu_core::Error::not_found("parameter", name));
        }
        self.except_if_wrong_parameter(name)?;
        if self.readable().contains(&name) {
            return Err(akantu_core::Error::invalid(format!(
                "parameter '{name}' of the non-linear solver {} is read-only",
                self.solver_id
            )));
        }
        format_value(&value, false)?;
        self.options
            .push((name.to_owned(), StoredOption::Value(value)));
        Ok(())
    }

    fn param_names(&self) -> Vec<&'static str> {
        self.readable().to_vec()
    }

    fn param_info(&self, name: &str) -> Option<akantu_parser::param::ParamInfo> {
        use akantu_parser::param::{ParamInfo, ParamKind, ParameterAccess};
        let kind = match name {
            "n_iter" | "max_iterations" => ParamKind::Int,
            "convergence_reason" => ParamKind::String,
            "convergence_type" => ParamKind::Enum,
            _ => ParamKind::Real,
        };
        // TAO's three are registered `_pat_parsmod` upstream (SNES's six are
        // read-only); `set` refuses them all the same.
        let access = if self.kind == Some(NonLinearSolverType::PetscTao) {
            ParameterAccess::PARSMOD
        } else {
            ParameterAccess::READABLE
        };
        self.readable()
            .contains(&name)
            .then_some(ParamInfo { kind, access })
    }
}

impl NonLinearSolver for NativeNonLinear {
    fn kind(&self) -> NonLinearSolverType {
        self.kind.unwrap_or(NonLinearSolverType::PetscSnes)
    }

    fn parse_options_section(
        &mut self,
        section: &akantu_parser::ParserSection,
    ) -> Option<Result<()>> {
        self.kind?;
        // Upstream `ParsablePETSc::parseSection`: every parameter checked
        // against the refused names, then (permissive parse) registered
        // read-only parameters are an error, and every parameter becomes a
        // PETSc option `-<name> <raw value>`.
        let r = (|| -> Result<()> {
            let params = section.parameters_sorted();
            for p in &params {
                self.except_if_wrong_parameter(&p.name.to_string_lossy())?;
            }
            for p in &params {
                let name = p.name.to_string_lossy().into_owned();
                if self.readable().contains(&name.as_str()) {
                    return Err(akantu_core::Error::invalid(format!(
                        "parameter '{name}' of the non-linear solver {} is not parsable",
                        self.solver_id
                    )));
                }
            }
            for p in params {
                let name = p.name.to_string_lossy().into_owned();
                let value = p.value.to_string_lossy().trim().to_owned();
                self.options.push((name, StoredOption::Raw(value)));
            }
            Ok(())
        })();
        Some(r)
    }

    fn solve(&mut self, dofs: &mut DofManager, callback: &mut dyn SolverCallback) -> Result<()> {
        let id = self.matrix_id.clone();
        let request = crate::backend::NativeRequest {
            kind: self.kind,
            solver_id: self.solver_id.clone(),
            options: self.options_formatted(dofs.upstream_compat().petsc_six_digit_real_options),
            bounds: self.bounds.clone(),
            upstream_aliasing: dofs.upstream_compat().petsc_aliased_solver_vectors,
        };
        let kind = self.kind;
        // UD-1022 (D-088, category A): bounds that exclude the zero
        // increment on an owned row are refused, agreed over the ranks,
        // before any callback runs (the model is left unchanged); so is a
        // NaN bound, which no comparison admits
        // (upstream hands it to TaoSetVariableBounds; ±inf bounds are
        // admitted)
        let ud1022_check = (kind == Some(NonLinearSolverType::PetscTao)
            && !dofs.upstream_compat().tao_bounds_excluding_zero_increment)
            .then_some(self.bounds.as_ref())
            .flatten();
        let solver_id = self.solver_id.clone();
        let stale = self.stale_bounds.clone();
        let (iterations, error, converged, report) =
            dofs.collective(&format!("native-solve:{id}"), |dofs| {
                // Bounds first: bounds set before a renumbering name other
                // equations; refused (agreed, recoverable) before anything
                // of the solve runs, and before the UD-1022 count below,
                // which would read them against the renumbered rows
                dofs.step("native_bounds", |_| match &stale {
                    Some(cause) => Err(akantu_core::Error::invalid(format!(
                        "native non-linear solver: the variable bounds were set before {cause}; \
                         they name the old local equations: set them again \
                         (NativeNonLinear::set_bounds) before the next bounded solve"
                    ))),
                    None => Ok(()),
                })?;
                if let Some((lower, upper)) = ud1022_check {
                    let rows = dofs.owned_count("ud1022-bounds", lower, upper, |lo, up| {
                        !(lo <= 0.0 && up >= 0.0)
                    })?;
                    if rows > 0 {
                        return Err(akantu_core::Error::Unsupported(format!(
                            "petsc_tao solver '{solver_id}': the bounds exclude the zero \
                             increment (lower > 0, upper < 0, or a NaN bound) on {rows} owned \
                             row(s) of the job; refused (UD-1022, D-088): upstream's TAO objective is exact \
                             only at a zero increment and GPCG evaluates it at the projection \
                             of 0 onto the bounds, so the solve would minimise a shifted \
                             functional and return a wrong result; set the DOF manager's \
                             upstream-compatibility switch \
                             tao_bounds_excluding_zero_increment to reproduce upstream"
                        )));
                    }
                }
                dofs.step("before_solve_step", |d| callback.before_solve_step(d))?;
                dofs.update_global_blocked_dofs();
                // Upstream NonLinearSolverTAO assembles J before the
                // predictor, NonLinearSolverPETSc after it.
                if kind == Some(NonLinearSolverType::PetscTao) {
                    dofs.step("callback:assemble_matrix", |d| {
                        callback.assemble_matrix(d, &id)
                    })?;
                }
                dofs.step("predictor", |d| callback.predictor(d))?;
                if kind == Some(NonLinearSolverType::PetscSnes) {
                    dofs.step("callback:assemble_matrix", |d| {
                        callback.assemble_matrix(d, &id)
                    })?;
                }
                let mut driver = dofs.step("native_driver", |d| {
                    d.backend_mut()?.native_non_linear_with(&id, &request)
                })?;
                let outcome = {
                    let mut ctx = crate::NativeSolveContext::new(dofs, callback, id.clone());
                    let out = driver.solve(&mut ctx);
                    // A trampoline error keeps its diagnostic priority over
                    // the driver status (the C side only saw a nonzero
                    // code), but never hides a more urgent fatal kind of
                    // the driver's own outcome (a native
                    // backend detecting an unusable state while tearing
                    // down a failed solve). `agree_outcome` then classifies
                    // the combined outcome against the scope's state.
                    match (ctx.take_error(), out) {
                        (None, out) => out,
                        (Some(e), Ok(_)) => Err(e),
                        (Some(e), Err(driver)) => Err(e.escalated_by(&driver)),
                    }
                };
                let report = driver.report();
                let (iterations, error, converged) =
                    dofs.agree_outcome("native_outcome", outcome)?;
                dofs.step("after_solve_step", |d| {
                    callback.after_solve_step(d, converged)
                })?;
                Ok((iterations, error, converged, report))
            })?;
        self.iterations = iterations;
        self.error = error;
        self.converged = converged;
        // Upstream SNESNotConvergedException(rtol, n_iter, norm / a[0]);
        // TAO: the absolute gradient tolerance and the final
        // gradient norm TAO compared with it (upstream's
        // TAONotConvergedException text: `NativeReport::
        // tao_not_converged_message`, through `report()`).
        let (threshold, relative) = match (kind, report.as_ref()) {
            (Some(NonLinearSolverType::PetscTao), Some(r)) => (r.atol, r.function_norm),
            (_, Some(r)) => (r.rtol, r.relative_norm),
            (_, None) => (f64::NAN, error),
        };
        self.report = report;
        if converged {
            Ok(())
        } else {
            Err(akantu_core::Error::NotConverged {
                threshold,
                niter: iterations,
                error: relative,
            })
        }
    }

    fn nb_iterations(&self) -> usize {
        self.iterations
    }

    fn error(&self) -> f64 {
        self.error
    }

    fn converged(&self) -> bool {
        self.converged
    }

    fn as_native_mut(&mut self) -> Option<&mut NativeNonLinear> {
        Some(self)
    }
}
