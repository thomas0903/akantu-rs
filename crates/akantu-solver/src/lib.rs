// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/{dof_manager/,
// non_linear_solver/, time_step_solvers/, integration_scheme/,
// model_solver.hh, model_solver.cc, solver_callback.hh}.

//! # akantu-solver
//!
//! The solver infrastructure shared by all models: the DOF manager, the
//! non-linear and time-step solvers and the integration schemes, ported
//! from upstream's `src/model/common` (`dof_manager`, `non_linear_solver`,
//! `time_step_solvers`, `integration_scheme`, `model_solver`). Cargo
//! features, both off by default: `mumps` and `petsc` (the PETSc DOF-manager
//! back-end and the SNES and TAO solvers), which enable the features of the
//! same names of `akantu-linalg`.
//!
//! Ownership (decision D-021). Upstream forms a cycle:
//! `Model` *is a* `SolverCallback`, owns (via `ModelSolver`) a `DOFManager`
//! that owns the `TimeStepSolver`s and `NonLinearSolver`s, which call back
//! into the model. In Rust:
//!
//! * [`dof_manager::DofManager`] owns the DOF arrays (displacements,
//!   velocities, accelerations, increments, previous values, blocked flags),
//!   the residual, the sparse and lumped matrices and the equation numbering.
//! * [`model_solver::ModelSolver`] owns one `DofManager` and the named
//!   [`time_step::TimeStepSolver`]s (each owning its non-linear solver and
//!   integration schemes).
//! * Physics types implement [`callback::SolverCallback`], whose methods
//!   receive the `&mut DofManager` as a parameter. A model struct keeps
//!   its physics state and its `ModelSolver` in separate fields, so
//!   `solver.solve_step(&mut physics)` is a plain disjoint borrow. Coupled
//!   models are either monolithic (one shared `ModelSolver`, combined
//!   physics) or staggered (independent component solvers), see
//!   `akantu_model::coupling` (D-021 revised).
//! * [`backend::LinearSystemBackend`]: matrix storage/assembly/finalisation
//!   (AIJ default; `petsc_backend::PetscBackend` with the `petsc` feature)
//!   behind the DOF manager.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod backend;
pub mod callback;
pub mod dof_manager;
pub mod dof_synchronizer;
pub mod format;
pub mod integration_scheme;
pub mod model_solver;
pub mod non_linear;
pub mod non_linear_lumped;
pub mod non_linear_newton_raphson;
#[cfg(feature = "petsc")]
pub mod petsc_backend;
pub mod time_step;

pub use backend::{
    AijBackend, BackendContext, BackendKind, LinearSystemBackend, MatrixAddPolicy,
    NativeNonLinearDriver, NativeReport, NativeRequest, NativeSolveContext,
};
pub use callback::SolverCallback;
pub use dof_manager::{
    ConvergenceVector, DofData, DofManager, FamilyArray, LumpedBlockCouplings, ScopeGuard,
    UpstreamCompat,
};
pub use integration_scheme::{
    GeneralizedTrapezoidal, IntegrationScheme, NewmarkBeta, PseudoTime, SolutionType,
    new_integration_scheme,
};
pub use model_solver::{ModelSolver, ModelSolverOptions};
pub use non_linear::{NativeNonLinear, NonLinearSolver, new_non_linear_solver};
pub use non_linear_lumped::NonLinearSolverLumped;
pub use non_linear_newton_raphson::NonLinearSolverNewtonRaphson;
pub use time_step::TimeStepSolver;
