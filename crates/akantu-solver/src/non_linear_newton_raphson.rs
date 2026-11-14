// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_linear_solver/
// {non_linear_solver.hh, non_linear_solver.cc,
// non_linear_solver_newton_raphson.hh, non_linear_solver_newton_raphson.cc}.

//! Newton–Raphson family (NLS-003…005, NLS-010…014): `newton_raphson`,
//! `newton_raphson_modified`, `newton_raphson_contact` and the
//! single-iteration `linear` solver (upstream `NonLinearSolverLinear`).
//!
//! # Upstream semantics (reproduced)
//!
//! * Parameters `type`, `threshold` (1e-10), `convergence_type`
//!   (`solution`), `max_iterations` (10), read-only `error`,
//!   `nb_iterations`, `converged`, `force_linear_recompute` (true),
//!   `verbose_iteration` (false), registered in upstream order.
//! * `solve`: `before_solve_step`, blocked snapshot, `predictor`, residual
//!   (split as `external − K·u + inertial` for the `linear` type when the
//!   callback can split it), Jacobian once before the loop (`modified`,
//!   or `linear` with `force_linear_recompute`); with `convergence_type =
//!   residual` an initial test against the absolute threshold, then the
//!   threshold becomes `error · threshold`; the loop (a `do … while`,
//!   at least one iteration) assembles `J` (`newton_raphson`, `contact`),
//!   solves, corrects, tests the residual (re-assembled) or the solution
//!   norm, re-assembles the residual after a failed solution test,
//!   counts the iteration and stops when converged or at
//!   `max_iterations`; then (solution criterion) a final residual
//!   assembly, `converged &= !(n > max)`, `after_solve_step(converged)`,
//!   and `Error::NotConverged` when not converged.
//! * `residual_mass_wgh` (NLS-014) was never implemented upstream: the
//!   solution-norm branch runs without the residual refreshes, so every
//!   non-converged iteration re-applies the first increment and a converged
//!   step leaves the residual stale (**UD-194**, D-003 B).
//!   The port refuses it by default (`Error::NotImplemented`, agreed
//!   on every rank before any callback);
//!   [`crate::UpstreamCompat::residual_mass_wgh_unrefreshed`] reproduces
//!   upstream for differential runs.
//! * Norm: [`DofManager::convergence_norm`] (unblocked local-or-master
//!   equations, `error < threshold`).
//!
//! # UD-010
//!
//! Upstream returns from a step whose initial residual already satisfies
//! the residual criterion without calling `after_solve_step`. The port
//! completes the successful-step lifecycle (`after_solve_step(true)`, no
//! corrector: nothing was solved; the time-step adapter then refreshes the
//! DOF increments) by default;
//! [`NonLinearSolverNewtonRaphson::set_legacy_hooks`] restores upstream's
//! early return for differential runs.
//!
//! # Collective protocol (`docs/ARCHITECTURE.md`)
//!
//! The whole solve is one [`DofManager::collective`] operation; every
//! callback runs as a [`DofManager::step`] (outcome agreed), the linear
//! solve and the norms are collective methods whose results are common
//! knowledge, so iteration counts, errors and the convergence decision are
//! identical on every rank and a rank-specific callback failure is
//! reported on every rank.
//!
//! # `dblTrace` dumps (DOF-017, D-081)
//!
//! Upstream compiles, in every build without `AKANTU_NDEBUG` (the pinned
//! oracle included), debugging dumps of the Newton loop that run only at
//! debug level `dblTrace` (`non_linear_solver_newton_raphson.cc:101-108,
//! 147-163`); the port runs them when [`akantu_core::debug::test_level`]
//! accepts [`DebugLevel::TRACE`] for this crate:
//!
//! 1. once per **process**, after the predictor of the first solve that
//!    finds the trace level active: `blip` on standard output and
//!    [`DofManager::save_associated_nodes_numbered`] to `nodes.mtx`;
//! 2. in every iteration, before the linear solve, the Jacobian `J` to
//!    `A_<k>.mtx` and the residual to `b_<k>.mtx`; after it the solution
//!    to `x_<k>.mtx`; `k` is one counter for the whole process, shared by
//!    every Newton solver and advanced at **every** iteration whether the
//!    trace is active or not (upstream's function-local `static int
//!    dump_counter`, incremented outside the level test), so two models
//!    interleave their numbers;
//! 3. files in the current directory, `_rank-<r>` before `.mtx` with
//!    several processes (the `saveMatrix` / `saveVector` naming).
//!
//! Values are written with the port's default Matrix Market policy (17
//! significant digits, UD-061 B);
//! [`NonLinearSolverNewtonRaphson::set_trace_matrix_market_options`] with
//! [`MatrixMarketOptions::upstream_precision`] gives upstream's bytes.
//! The `linear`-flagged solver (upstream `NonLinearSolverLinear`,
//! `solve_linear`) has no dumps, as upstream.

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use akantu_core::debug::DebugLevel;
use akantu_linalg::matrix_market::MatrixMarketOptions;

use akantu_core::enums::{NonLinearSolverType, SolveConvergenceCriteria, SparseSolverType};
use akantu_core::{Error, Id, Real, Result};
use akantu_linalg::direct::SparseSolverFactory;
use akantu_parser::ParserType;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    Parsable, table_get, table_info, table_names, table_set, table_set_from_input,
};

use crate::callback::SolverCallback;
use crate::dof_manager::{ConvergenceVector, DofManager};
use crate::non_linear::NonLinearSolver;

/// Newton–Raphson solver (upstream `NonLinearSolverNewtonRaphson`; with
/// `linear = true` upstream `NonLinearSolverLinear`).
#[derive(Debug)]
pub struct NonLinearSolverNewtonRaphson {
    id: Id,
    non_linear_solver_type: NonLinearSolverType,
    convergence_criteria_type: SolveConvergenceCriteria,
    convergence_criteria: Real,
    convergence_criteria_normalized: Real,
    max_iterations: i64,
    n_iter: i64,
    error: Real,
    converged: bool,
    force_linear_recompute: bool,
    verbose_iteration: bool,
    linear: bool,
    legacy_hooks: bool,
    matrix_id: Id,
    /// This solver's sparse direct solver: its resolved kind and whether
    /// it was allocated for the backend's rank-0 centralised execution
    /// (`None`: the backend solves with its own linear solver).
    sparse_solver: Option<(SparseSolverType, bool)>,
    /// Matrix Market policy of the `dblTrace` dumps (module docs).
    trace_matrix_market: MatrixMarketOptions,
}

/// Upstream's function-local `static bool first` of the `dblTrace` block
/// (`non_linear_solver_newton_raphson.cc:102`): one `blip` and `nodes.mtx`
/// per process.
static TRACE_FIRST: AtomicBool = AtomicBool::new(true);

/// Upstream's function-local `static int dump_counter`
/// (`non_linear_solver_newton_raphson.cc:148`): one counter for every
/// Newton solver of the process, advanced at every iteration.
static TRACE_DUMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Upstream `AKANTU_DEBUG_TEST(dblTrace)` for this crate.
fn trace_active() -> bool {
    akantu_core::debug::may_print(DebugLevel::TRACE)
        && akantu_core::debug::test_level(
            DebugLevel::TRACE,
            akantu_core::debug::module_of(module_path!()),
        )
}

const SUPPORTED: [NonLinearSolverType; 4] = [
    NonLinearSolverType::NewtonRaphsonModified,
    NonLinearSolverType::NewtonRaphsonContact,
    NonLinearSolverType::NewtonRaphson,
    NonLinearSolverType::Linear,
];

impl NonLinearSolverNewtonRaphson {
    /// New solver `id` of type `ty` (one of the Newton–Raphson types,
    /// `linear` or `auto`; upstream `checkIfTypeIsSupported`) whose sparse
    /// solver for `"J"` is installed in the manager's backend (upstream
    /// `SparseSolverFactory::allocate(type, dof_manager, "J")`). Backends
    /// that solve with their own linear solver (no pluggable direct solver)
    /// keep it. `linear = true` gives upstream `NonLinearSolverLinear`.
    ///
    /// `sparse` follows D-040: `Auto` resolves like upstream (MUMPS if
    /// registered — feature `mumps` — and the DOF manager's communicator is
    /// an MPI one, else `Eigen`, never `Faer`;
    /// `SparseSolverFactory::resolve_available`); the allocated backend has
    /// exactly the resolved kind. On several processes a backend that
    /// centralises serial solvers on rank 0
    /// ([`crate::LinearSystemBackend::centralizes_sparse_solves`], the AIJ
    /// backend) gets an `eigen`/`faer` solver from
    /// `SparseSolverFactory::create_centralized`; otherwise (MUMPS, which
    /// takes the distributed system itself) the direct `create(type,
    /// nb_proc)` applies, which refuses the serial-only `eigen`/`faer` on
    /// several processes like upstream.
    pub fn new(
        dofs: &mut DofManager,
        id: &str,
        ty: NonLinearSolverType,
        sparse: SparseSolverType,
        linear: bool,
    ) -> Result<Self> {
        if !SUPPORTED.contains(&ty) && ty != NonLinearSolverType::Auto {
            return Err(Error::invalid(format!(
                "The resolution method {ty} is not implemented in the non linear solver {id}!"
            )));
        }
        let factory = SparseSolverFactory::default();
        // D-040: `auto` → MUMPS when registered and the manager
        // communicates over MPI (the parallel MUMPS library needs it).
        let kind = factory.resolve_available(sparse, dofs.communicator().is_mpi());
        let sparse_solver = if dofs.backend().kind() == crate::backend::BackendKind::Petsc {
            // Upstream `SparseSolverFactory::allocate(_petsc, …)` creates a
            // `SparseSolverPETSc`; the PETSc backend solves `J` with its own
            // KSP. Any other sparse solver kind is refused, like
            // upstream's "Cannot use sparse solver of type …". Checked before
            // MUMPS's distributed dispatch.
            if kind != SparseSolverType::Petsc {
                return Err(Error::invalid(format!(
                    "Cannot use sparse solver of type {kind} with a PETSc DOF manager"
                )));
            }
            None
        } else {
            // Only the serial-only backends go through the rank-0
            // centralised execution; MUMPS takes the distributed system
            // itself.
            let centralized = dofs.communicator().size() > 1
                && dofs.backend().centralizes_sparse_solves()
                && matches!(kind, SparseSolverType::Eigen | SparseSolverType::Faer);
            let solver = Self::allocate_sparse_solver(&factory, kind, centralized, dofs)?;
            match dofs.backend_mut()?.set_sparse_solver("J", solver) {
                Ok(()) => Some((kind, centralized)),
                Err(Error::Unsupported(_)) => None,
                Err(e) => return Err(e),
            }
        };
        Ok(Self {
            id: id.into(),
            non_linear_solver_type: ty,
            convergence_criteria_type: SolveConvergenceCriteria::Solution,
            convergence_criteria: 1e-10,
            convergence_criteria_normalized: 1e-10,
            max_iterations: 10,
            n_iter: 0,
            error: 0.0,
            converged: false,
            force_linear_recompute: true,
            verbose_iteration: false,
            linear,
            legacy_hooks: false,
            matrix_id: "J".into(),
            sparse_solver,
            trace_matrix_market: MatrixMarketOptions::default(),
        })
    }

    fn allocate_sparse_solver(
        factory: &SparseSolverFactory,
        kind: SparseSolverType,
        centralized: bool,
        dofs: &DofManager,
    ) -> Result<Box<dyn akantu_linalg::direct::SparseDirectSolver>> {
        if centralized {
            factory.create_centralized(kind)
        } else {
            factory.create(kind, dofs.communicator().size())
        }
    }

    /// Upstream: every Newton–Raphson solver owns its sparse solver on
    /// `"J"`. The backend keeps one solver per matrix, so a solver sharing
    /// the DOF manager with another one re-installs its own choice before
    /// solving (local; the choice is identical on every rank).
    fn install_sparse_solver(&self, dofs: &mut DofManager) -> Result<()> {
        let Some((kind, centralized)) = self.sparse_solver else {
            return Ok(());
        };
        if dofs.backend().sparse_solver_kind(&self.matrix_id) != Some(kind) {
            let factory = SparseSolverFactory::default();
            let solver = Self::allocate_sparse_solver(&factory, kind, centralized, dofs)?;
            dofs.backend_mut()?
                .set_sparse_solver(&self.matrix_id, solver)?;
        }
        Ok(())
    }

    /// Set an option of this solver's linear solver (upstream
    /// `getSparseSolver().set(name, value)`; the PETSc backend: a KSP
    /// option such as `pc_type`, applied at the next solve). Backends
    /// without solver options return `Error::Unsupported`.
    pub fn set_sparse_solver_option(
        &self,
        dofs: &mut DofManager,
        name: &str,
        value: &str,
    ) -> Result<()> {
        dofs.backend_mut()?
            .set_solver_option(&self.matrix_id, name, value)
    }

    /// Kind of this solver's sparse direct solver (`None`: the backend
    /// solves with its own linear solver).
    #[must_use]
    pub fn sparse_solver_kind(&self) -> Option<SparseSolverType> {
        self.sparse_solver.map(|(k, _)| k)
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// UD-010: `true` restores upstream's early return without
    /// `after_solve_step` when the initial residual already converges
    /// (legacy hook mode for differential runs); default `false`.
    pub fn set_legacy_hooks(&mut self, legacy: bool) {
        self.legacy_hooks = legacy;
    }

    /// Matrix Market policy of the `dblTrace` dumps (default: 17
    /// significant digits, UD-061;
    /// [`MatrixMarketOptions::upstream_precision`] for upstream's bytes).
    pub fn set_trace_matrix_market_options(&mut self, options: MatrixMarketOptions) {
        self.trace_matrix_market = options;
    }

    /// Once per process (upstream `static bool first`): `blip` and the
    /// associated nodes (`nodes.mtx`).
    fn trace_first(&self, dofs: &mut DofManager) -> Result<()> {
        if trace_active() && TRACE_FIRST.swap(false, Ordering::SeqCst) {
            let mut out = std::io::stdout();
            let _ = out.write_all(b"blip\n").and_then(|()| out.flush());
            dofs.save_associated_nodes_numbered(Path::new("nodes"), &self.trace_matrix_market)?;
        }
        Ok(())
    }

    /// Before the linear solve of iteration `k`: `A_<k>` (the Jacobian) and
    /// `b_<k>` (the residual).
    fn trace_before_solve(&self, dofs: &DofManager, k: usize) -> Result<()> {
        if !trace_active() {
            return Ok(());
        }
        let (rank, size) = (dofs.communicator().rank(), dofs.communicator().size());
        dofs.matrix(&self.matrix_id)?.save_matrix_on_with(
            Path::new(&format!("A_{k}")),
            rank,
            size,
            &self.trace_matrix_market,
        )?;
        // the residual is exportable storage (`ResidualVector`):
        // dumped through a copy of the general vector type that keeps its
        // kind (on a distributed mesh with several
        // processes every rank writes its own `b_<k>_rank-<r>.mtx`,
        // upstream `SolverVectorDistributed::saveVector`)
        let b = dofs.residual().to_solver_vector();
        b.save_vector_with(
            Path::new(&format!("b_{k}")),
            dofs.layout(),
            rank,
            size,
            &self.trace_matrix_market,
        )?;
        Ok(())
    }

    /// After the linear solve of iteration `k`: `x_<k>` (the solution, a
    /// vector of the manager's kind, as upstream's `getSolution()`:
    /// [`DofManager::vectors_distributed`]).
    fn trace_after_solve(&self, dofs: &DofManager, k: usize) -> Result<()> {
        if !trace_active() {
            return Ok(());
        }
        let (rank, size) = (dofs.communicator().rank(), dofs.communicator().size());
        let mut x = akantu_linalg::SolverVector::new("solution", dofs.vectors_distributed());
        x.values = dofs.solution().to_vec();
        x.save_vector_with(
            Path::new(&format!("x_{k}")),
            dofs.layout(),
            rank,
            size,
            &self.trace_matrix_market,
        )?;
        Ok(())
    }

    /// Threshold the last test compared against (upstream
    /// `convergence_criteria_normalized`).
    #[must_use]
    pub fn normalized_threshold(&self) -> Real {
        self.convergence_criteria_normalized
    }

    /// Upstream `NonLinearSolver::assembleResidual`.
    fn assemble_residual(&self, dofs: &mut DofManager, cb: &mut dyn SolverCallback) -> Result<()> {
        if cb.can_split_residual() && self.non_linear_solver_type == NonLinearSolverType::Linear {
            dofs.zero_residual();
            dofs.step("assemble_residual:external", |d| {
                cb.assemble_residual_part(d, "external")
            })?;
            dofs.assemble_mat_mul_dofs_to_residual("K", -1.0)?;
            dofs.step("assemble_residual:inertial", |d| {
                cb.assemble_residual_part(d, "inertial")
            })
        } else {
            dofs.step("assemble_residual", |d| cb.assemble_residual(d))
        }
    }

    fn assemble_jacobian(&self, dofs: &mut DofManager, cb: &mut dyn SolverCallback) -> Result<()> {
        let id = self.matrix_id.clone();
        dofs.step("assemble_matrix:J", |d| cb.assemble_matrix(d, &id))
    }

    /// Upstream `testConvergence` (sets `error`).
    fn test_convergence(&mut self, dofs: &mut DofManager, of: ConvergenceVector) -> Result<bool> {
        self.error = dofs.convergence_norm(of)?;
        Ok(self.error < self.convergence_criteria_normalized)
    }

    fn verbose(&self, dofs: &DofManager) {
        if !self.verbose_iteration || dofs.communicator().rank() != 0 {
            return;
        }
        let _ = std::io::stdout().write_all(self.verbose_iteration_line().as_bytes());
    }

    /// The line upstream prints after an iteration when `verbose_iteration`
    /// is set (on rank 0 only; `non_linear_solver_newton_raphson.cc:186-197`,
    /// DOF-017):
    ///
    /// ```text
    /// [<criteria type>] Convergence iteration <n_iter>: error <error> <|> <normalized> [<criteria>/<normalized ÷ criteria>]
    /// ```
    ///
    /// `n_iter` right-aligned in `std::setw(std::log10(max_iterations))`
    /// (the `double` converted to `int`, so 10 → 1, 99 → 1, 100 → 2; 0 for
    /// a non-positive maximum, where upstream's `log10` gives `-inf`, an
    /// undefined conversion); every `Real` after `std::scientific` with the
    /// stream's default precision 6 (`%.6e`: at least two exponent digits);
    /// `<` when the iteration converged. Upstream's `std::scientific` stays
    /// set on `std::cout` afterwards (it does not change this line).
    #[must_use]
    pub fn verbose_iteration_line(&self) -> String {
        let width = if self.max_iterations > 0 {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            // upstream std::setw(std::log10(max_iterations)): double → int
            let w = (self.max_iterations as f64).log10() as usize;
            w
        } else {
            0
        };
        let norm = self.convergence_criteria_normalized;
        format!(
            "[{}] Convergence iteration {:>width$}: error {} {} {} [{}/{}]\n",
            self.convergence_criteria_type,
            self.n_iter,
            crate::format::c_scientific(self.error, 6),
            if self.converged { "<" } else { ">" },
            crate::format::c_scientific(norm, 6),
            crate::format::c_scientific(self.convergence_criteria, 6),
            crate::format::c_scientific(norm / self.convergence_criteria, 6),
        )
    }

    fn solve_nr(&mut self, dofs: &mut DofManager, cb: &mut dyn SolverCallback) -> Result<()> {
        use NonLinearSolverType as T;
        use SolveConvergenceCriteria as C;
        let ty = self.non_linear_solver_type;
        dofs.step("before_solve_step", |d| cb.before_solve_step(d))?;
        dofs.update_global_blocked_dofs();
        dofs.step("predictor", |d| cb.predictor(d))?;
        self.trace_first(dofs)?;
        if ty == T::Linear && cb.can_split_residual() {
            dofs.step("assemble_matrix:K", |d| cb.assemble_matrix(d, "K"))?;
        }
        self.assemble_residual(dofs, cb)?;
        if ty == T::NewtonRaphsonModified || (ty == T::Linear && self.force_linear_recompute) {
            self.assemble_jacobian(dofs, cb)?;
            self.force_linear_recompute = false;
        }
        self.n_iter = 0;
        self.converged = false;
        self.convergence_criteria_normalized = self.convergence_criteria;
        if self.convergence_criteria_type == C::Residual {
            self.converged = self.test_convergence(dofs, ConvergenceVector::Residual)?;
            if self.converged {
                if !self.legacy_hooks {
                    dofs.step("after_solve_step", |d| cb.after_solve_step(d, true))?;
                }
                return Ok(());
            }
            self.convergence_criteria_normalized = self.error * self.convergence_criteria;
        }
        loop {
            if ty == T::NewtonRaphson || ty == T::NewtonRaphsonContact {
                self.assemble_jacobian(dofs, cb)?;
            }
            let k = TRACE_DUMP_COUNTER.load(Ordering::SeqCst);
            self.trace_before_solve(dofs, k)?;
            dofs.solve(&self.matrix_id)?;
            self.trace_after_solve(dofs, k)?;
            TRACE_DUMP_COUNTER.fetch_add(1, Ordering::SeqCst);
            dofs.step("corrector", |d| cb.corrector(d))?;
            if self.convergence_criteria_type == C::Residual {
                self.assemble_residual(dofs, cb)?;
                self.converged = self.test_convergence(dofs, ConvergenceVector::Residual)?;
            } else {
                self.converged = self.test_convergence(dofs, ConvergenceVector::Solution)?;
            }
            if self.convergence_criteria_type == C::Solution && !self.converged {
                self.assemble_residual(dofs, cb)?;
            }
            self.n_iter += 1;
            self.verbose(dofs);
            if self.converged || self.n_iter >= self.max_iterations {
                break;
            }
        }
        if self.convergence_criteria_type == C::Solution {
            self.assemble_residual(dofs, cb)?;
        }
        self.converged = self.converged && !(self.n_iter > self.max_iterations);
        let converged = self.converged;
        dofs.step("after_solve_step", |d| cb.after_solve_step(d, converged))
    }

    /// Upstream `solve_linear` (one solve; `n_iter`, `error`, `converged`
    /// untouched, as upstream).
    fn solve_linear(&mut self, dofs: &mut DofManager, cb: &mut dyn SolverCallback) -> Result<()> {
        dofs.step("before_solve_step", |d| cb.before_solve_step(d))?;
        dofs.update_global_blocked_dofs();
        dofs.step("predictor", |d| cb.predictor(d))?;
        self.assemble_jacobian(dofs, cb)?;
        // Residual computed after J to allow the model to use K to compute
        // the residual.
        self.assemble_residual(dofs, cb)?;
        dofs.solve(&self.matrix_id)?;
        dofs.step("corrector", |d| cb.corrector(d))?;
        if cb.can_split_residual() {
            dofs.step("assemble_residual:internal", |d| {
                cb.assemble_residual_part(d, "internal")
            })?;
        } else {
            self.assemble_residual(dofs, cb)?;
        }
        dofs.step("after_solve_step", |d| cb.after_solve_step(d, true))
    }
}

static NR_PARAMS: &[ParamDescriptor<NonLinearSolverNewtonRaphson>] = &[
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
        "threshold",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "Threshold to consider results as converged",
        |s| ParamValue::Real(s.convergence_criteria),
        Some(|s, v| {
            s.convergence_criteria = v.as_real()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "convergence_type",
        ParamKind::Enum,
        ParameterAccess::PARSMOD,
        "Type of convergence criteria",
        |s| ParamValue::Enum(s.convergence_criteria_type.to_string()),
        Some(|s, v| {
            s.convergence_criteria_type = parse_enum(&v)?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "max_iterations",
        ParamKind::Int,
        ParameterAccess::PARSMOD,
        "Max number of iterations",
        |s| ParamValue::Int(s.max_iterations),
        Some(|s, v| {
            s.max_iterations = v.as_int()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "error",
        ParamKind::Real,
        ParameterAccess::READABLE,
        "Last reached error",
        |s| ParamValue::Real(s.error),
        None,
    ),
    ParamDescriptor::new(
        "nb_iterations",
        ParamKind::Int,
        ParameterAccess::READABLE,
        "Last reached number of iterations",
        |s| ParamValue::Int(s.n_iter),
        None,
    ),
    ParamDescriptor::new(
        "converged",
        ParamKind::Bool,
        ParameterAccess::READABLE,
        "Did last solve converged",
        |s| ParamValue::Bool(s.converged),
        None,
    ),
    ParamDescriptor::new(
        "force_linear_recompute",
        ParamKind::Bool,
        ParameterAccess::MODIFIABLE,
        "Force reassembly of the jacobian matrix",
        |s| ParamValue::Bool(s.force_linear_recompute),
        Some(|s, v| {
            s.force_linear_recompute = v.as_bool()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "verbose_iteration",
        ParamKind::Bool,
        ParameterAccess::MODIFIABLE,
        "Make internal iterations verbose",
        |s| ParamValue::Bool(s.verbose_iteration),
        Some(|s, v| {
            s.verbose_iteration = v.as_bool()?;
            Ok(())
        }),
    ),
];

/// Parse an enumeration parameter value (spelling or text).
pub(crate) fn parse_enum<T: std::str::FromStr<Err = Error>>(v: &ParamValue) -> Result<T> {
    match v {
        ParamValue::Enum(s) | ParamValue::String(s) => s.trim().parse(),
        ParamValue::ByteString(b) => std::str::from_utf8(b.as_bytes())
            .map_err(|_| Error::invalid("enumeration value is not valid UTF-8"))?
            .trim()
            .parse(),
        other => Err(Error::invalid(format!(
            "expected an enumeration value, got {:?}",
            other.kind()
        ))),
    }
}

impl ParameterRegistry for NonLinearSolverNewtonRaphson {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(NR_PARAMS, self, name)
    }
    fn set_param(&mut self, name: &str, v: ParamValue) -> Result<()> {
        table_set(NR_PARAMS, self, name, v)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(NR_PARAMS)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(NR_PARAMS, name)
    }
    fn set_param_from_input(&mut self, name: &str, v: ParamValue) -> Result<()> {
        table_set_from_input(NR_PARAMS, self, name, v)
    }
}

impl Parsable for NonLinearSolverNewtonRaphson {
    fn section_type(&self) -> ParserType {
        ParserType::NonLinearSolver
    }
    fn parsable_id(&self) -> &str {
        &self.id
    }
}

impl NonLinearSolver for NonLinearSolverNewtonRaphson {
    fn kind(&self) -> NonLinearSolverType {
        self.non_linear_solver_type
    }

    fn solve(&mut self, dofs: &mut DofManager, callback: &mut dyn SolverCallback) -> Result<()> {
        let name = format!("nls:{}", self.id);
        let installed = self.install_sparse_solver(dofs);
        if self.linear {
            return dofs.collective(&name, |dofs| {
                dofs.step("install_sparse_solver", |_| installed)?;
                self.solve_linear(dofs, callback)
            });
        }
        let criterion = if self.convergence_criteria_type
            == SolveConvergenceCriteria::ResidualMassWgh
            && !dofs.upstream_compat().residual_mass_wgh_unrefreshed
        {
            Err(Error::NotImplemented(
                "convergence_type = residual_mass_wgh was never implemented upstream (UD-194: \
                 the solution-norm branch without residual refreshes re-applies the first \
                 increment); UpstreamCompat::residual_mass_wgh_unrefreshed reproduces it",
            ))
        } else {
            Ok(())
        };
        dofs.collective(&name, |dofs| {
            dofs.step("install_sparse_solver", |_| installed)?;
            dofs.step("convergence_type", |_| criterion)?;
            self.solve_nr(dofs, callback)
        })?;
        if self.converged {
            Ok(())
        } else {
            Err(Error::NotConverged {
                threshold: self.convergence_criteria_normalized,
                niter: usize::try_from(self.n_iter).unwrap_or(0),
                error: self.error,
            })
        }
    }

    fn nb_iterations(&self) -> usize {
        usize::try_from(self.n_iter).unwrap_or(0)
    }

    fn error(&self) -> f64 {
        self.error
    }

    fn converged(&self) -> bool {
        self.converged
    }

    fn as_newton_raphson_mut(&mut self) -> Option<&mut NonLinearSolverNewtonRaphson> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use akantu_comm::Communicator;

    use super::*;

    fn solver(max_iterations: i64) -> NonLinearSolverNewtonRaphson {
        let mut dofs = DofManager::new("dm", Communicator::serial());
        let mut s = NonLinearSolverNewtonRaphson::new(
            &mut dofs,
            "nr",
            NonLinearSolverType::NewtonRaphson,
            SparseSolverType::Eigen,
            false,
        )
        .unwrap();
        s.max_iterations = max_iterations;
        s
    }

    /// DOF-017: the `verbose_iteration`
    /// text of upstream `non_linear_solver_newton_raphson.cc:186-197`,
    /// written out by hand from its iostream formatting: `std::setw` of the
    /// truncated `log10(max_iterations)`, `std::scientific` with precision
    /// 6 for every `Real`, `<` / `>` by convergence.
    #[test]
    fn verbose_iteration_line_is_upstreams_text() {
        let mut s = solver(10);
        s.convergence_criteria_type = SolveConvergenceCriteria::Residual;
        s.convergence_criteria = 1e-10;
        s.convergence_criteria_normalized = 2.5e-12;
        s.n_iter = 3;
        s.error = 1.234567e-5;
        s.converged = true;
        assert_eq!(
            s.verbose_iteration_line(),
            "[residual] Convergence iteration 3: error 1.234567e-05 < 2.500000e-12 \
             [1.000000e-10/2.500000e-02]\n"
        );
        let mut s = solver(100);
        s.convergence_criteria_type = SolveConvergenceCriteria::Solution;
        s.convergence_criteria = 1e-8;
        s.convergence_criteria_normalized = 1e-8;
        s.n_iter = 7;
        s.error = 3e-3;
        s.converged = false;
        assert_eq!(
            s.verbose_iteration_line(),
            "[solution] Convergence iteration  7: error 3.000000e-03 > 1.000000e-08 \
             [1.000000e-08/1.000000e+00]\n"
        );
        // the width: int(log10(max_iterations)), a minimum
        for (max, n, field) in [
            (99, 12, "12"),
            (1000, 5, "  5"),
            (1, 4, "4"),
            (0, 4, "4"),
            (-3, 4, "4"),
        ] {
            let mut s = solver(max);
            s.n_iter = n;
            let line = s.verbose_iteration_line();
            assert!(
                line.starts_with(&format!("[solution] Convergence iteration {field}: error ")),
                "max {max}: {line:?}"
            );
        }
        // the residual-weighted criterion's name and a large exponent
        let mut s = solver(10);
        s.convergence_criteria_type = SolveConvergenceCriteria::ResidualMassWgh;
        s.error = 6.02214076e123;
        s.n_iter = 1;
        assert!(
            s.verbose_iteration_line()
                .starts_with("[residual_mass_wgh] Convergence iteration 1: error 6.022141e+123 > "),
            "{}",
            s.verbose_iteration_line()
        );
    }
}
