// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_common/
// test_dof_manager.cc (the `dof_manager_petsc_t` instance of the typed
// DOFManagerFixture suite, TEST-070, DOF-015), src/model/common/
// non_linear_solver/non_linear_solver_tao.{hh,cc} (setBounds,
// TAONotConvergedException, the readable parameters), src/io/parser/
// parsable_petsc.hh (updateInternalParameters: the global options database
// applied after the private one), src/solver/sparse_solver_petsc.cc
// (the KSPSetUp failure "is your matrix singular ?").

//! Scenarios of the PETSc / TAO models in the PETSc tier (`petsc_mpi`, real MPI at 1 / 2
//! / 4 ranks): the PETSc instance of upstream's `test_dof_manager`,
//! bounded TAO solves, options set in PETSc's global
//! database after the initialisation and their precedence, the
//! KSP set-up failure.

pub mod cases;
pub mod dof_manager;
pub mod tao_bounds;
