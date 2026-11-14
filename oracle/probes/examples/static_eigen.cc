// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_model/static/static.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Change: the Eigen sparse solver is pinned.
// Eigen's cache sizes are pinned at the start of main (D-050).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
 * Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
 *
 * This file is part of Akantu
 *
 * Akantu is free software: you can redistribute it and/or modify it under the
 * terms of the GNU Lesser General Public License as published by the Free
 * Software Foundation, either version 3 of the License, or (at your option) any
 * later version.
 *
 * Akantu is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
 * A PARTICULAR PURPOSE. See the GNU Lesser General Public License for more
 * details.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with Akantu. If not, see <http://www.gnu.org/licenses/>.
 */

/* -------------------------------------------------------------------------- */
#include "non_linear_solver.hh"
#include "solid_mechanics_model.hh"
/* -------------------------------------------------------------------------- */
using namespace akantu;
/* -------------------------------------------------------------------------- */

// akantu-rs: pinned Eigen sparse solver (upstream `_auto` resolves to MUMPS
// in the oracle; the port's SparseLU translation is bitwise against `_eigen`)
static ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  initialize("material.dat", argc, argv);

  const Int spatial_dimension = 2;

  Mesh mesh(spatial_dimension);
  mesh.read("square.msh");

  SolidMechanicsModel model(mesh);

  /// model initialization
  model.initFull(_analysis_method = _static,
                 _solver_options = eigen_options()); // akantu-rs: Eigen

  /// configure dumper
  model.setBaseName("static");
  model.addDumpFieldVector("displacement");
  model.addDumpField("external_force");
  model.addDumpField("internal_force");
  model.addDumpField("blocked_dofs");
  model.addDumpField("grad_u");
  model.addDumpField("stress");

  /// Dirichlet boundary conditions
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "Fixed_x");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "Fixed_y");
  model.applyBC(BC::Dirichlet::FixedValue(0.0001, _x), "Displacement");
  model.dump();

  /// set the limits for the Newton-Raphson solver
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 2);
  solver.set("threshold", 2e-4);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);

  /// static solve
  model.solveStep();
  model.dump();

  return 0;
}
