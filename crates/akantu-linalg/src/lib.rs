// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/{sparse_matrix*.hh,
// sparse_matrix_aij*.hh, solver_vector*.hh, sparse_solver*.hh,
// terms_to_assemble.hh}, src/common/aka_types.hh (eig/eigh).
// The modules `dense::eigen` and `eigen_sparse_lu` are MPL-2.0 translations
// of Eigen 3.4.0 (see their headers, NOTICE.md and docs/DEPENDENCIES.md).

//! # akantu-linalg
//!
//! Numerical algebra independent of meshes (decisions D-014, D-040): sparse
//! matrices, the sparse direct solvers and dense decompositions, ported
//! from upstream's `src/solver` (matrices, vectors, sparse solvers) and the
//! eigen-solvers of `aka_types.hh`. Cargo features, both off by default:
//! `mumps` (the MUMPS solver; needs MPI and the MUMPS development files) and
//! `petsc` (bindings of PETSc; needs MPI and the PETSc development files).
//! Contents:
//!
//! * [`sparse::SparseMatrixAij`] (and the [`sparse::SparseMatrix`] trait):
//!   upstream's COO "AIJ" matrix over global equations: canonical
//!   lookup key, first-inserted stored coordinates, upstream profile order,
//!   value selection and release counters, UD-034 symmetry policy, checked
//!   1-based export (`irn`/`jcn`) for MUMPS and Python, Matrix Market
//!   output ([`matrix_market`]: 17 significant digits by default, UD-061;
//!   upstream's bytes with `MatrixMarketOptions::upstream_precision`).
//! * [`layout::EquationLayout`]: local ↔ global equations with ownership
//!   (owned / shared / pure ghost) and the persistent numbering offset.
//! * [`vector::SolverVector`]: residual / solution / lumped vectors;
//!   [`centralize`]: rank-0 gather/scatter of a distributed AIJ system.
//! * [`direct::SparseDirectSolver`] and [`direct::SparseSolverFactory`]
//!   (upstream names, D-040): `SparseSolverType::Eigen` / `eigen` is
//!   [`eigen_sparse_lu`], a translation of Eigen's `SparseLU`
//!   bit-identical with the oracle's strict build on valid finite input;
//!   `SparseSolverType::Faer` / `faer` is [`direct::FaerLuSolver`], the
//!   pure-Rust port extension, qualified by a conditioning-aware protocol
//!   (no bitwise claim; never chosen by `auto`, never substituted);
//!   `SparseSolverType::Mumps` / `mumps` is `mumps::MumpsSolver` (feature
//!   `mumps`: distributed assembled input, centralised RHS,
//!   [`direct::SparseDirectSolver::solve_distributed`]); PETSc solves are
//!   the PETSc DOF-manager backend. Every backend refuses
//!   non-finite systems and reports singular matrices and range failures
//!   as errors.
//! * `petsc` (feature `petsc`): safe bindings of PETSc (session,
//!   matrices, vectors, KSP, SNES, TAO, options databases, application
//!   ordering) over a C shim; the DOF-manager glue is
//!   `akantu_solver::petsc_backend`. Opt-in, off by default: a build with
//!   it links Debian's PETSc and its GPL-2+ dependencies and is distributed
//!   under GPL-3.0-or-later as a whole (D-076, `NOTICE.md`).
//! * [`terms::TermsToAssemble`]: pre-assembled terms (constraints, contact).
//! * [`dense`]: dense decompositions for dynamically sized matrices
//!   (symmetric/general eigen-solvers used by materials and dumpers).

// `unsafe` is confined to the native FFI edges (`mumps`, feature `mumps`;
// `petsc`, feature `petsc`) and to the run-time instruction-set dispatch of
// the SparseLU dense kernels (`eigen_sparse_lu::simd`); `UNSAFE.md`.
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod centralize;
pub mod dense;
pub mod direct;
pub mod eigen_sparse_lu;
pub mod int_hash;
pub mod layout;
pub mod matrix_market;
#[cfg(feature = "mumps")]
pub mod mumps;
// The PETSc FFI edge: every unsafe site carries its SAFETY note.
#[cfg(feature = "petsc")]
#[allow(unsafe_code)]
pub mod petsc;
pub mod sparse;
pub mod terms;
pub mod vector;

pub use direct::{SparseDirectSolver, SparseSolverFactory};
pub use layout::{EquationLayout, EquationOwnership, NoReduction, PartialSumReduction};
pub use sparse::{SparseMatrix, SparseMatrixAij, SymmetryPolicy};
pub use terms::TermsToAssemble;
pub use vector::SolverVector;
