// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/embedded_model/embedded.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: the Eigen sparse solver is pinned; Eigen's
// cache sizes are pinned at the start of main (D-050): the
// memory-safety precondition of UD-851 is computed on the background mesh
// and printed, and the program stops (exit 3) before building the model when
// it fails (goldens only on inputs where upstream is memory-safe). Built
// against the
// patched embedded oracle variant (D-060, oracle/patches/embedded,
// docs/ORACLE_BUILD.md) by oracle/probes/examples/embedded/CMakeLists.txt,
// run by oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2015-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
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
#include "embedded_interface_model.hh"
#include "non_linear_solver.hh"
/* -------------------------------------------------------------------------- */
#include <iostream>
#include <set> // akantu-rs: UD-851 precondition
/* -------------------------------------------------------------------------- */

using namespace akantu;

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

  const Int dim = 2;

  // Loading the concrete mesh
  Mesh mesh(dim);
  mesh.read("concrete.msh");

  // akantu-rs: UD-851 precondition. The reinforcement law's
  // element filter is initialised on this (background) mesh with dimension 1
  // (ConstitutiveLaw constructor + patch 0006), so it holds every regular
  // 1-D element type of this mesh, while the law's background filters exist
  // only for the interface mesh's types (_segment_2, the intersector's only
  // type); initBackgroundShapeDerivatives dereferences the null filter of
  // any other 1-D type. Memory-safe exactly when there is none.
  {
    std::set<ElementType> other_1d;
    for (auto gt : ghost_types) {
      for (auto type : mesh.elementTypes(1, gt, _ek_regular)) {
        if (type != _segment_2) {
          other_1d.insert(type);
        }
      }
    }
    std::cout << "akantu-rs UD-851 precondition: regular 1-D element types of the "
                 "background mesh other than _segment_2:";
    for (auto type : other_1d) {
      std::cout << " " << type;
    }
    std::cout << (other_1d.empty() ? " none: memory-safe" : ": violated, not run")
              << std::endl;
    if (not other_1d.empty()) {
      return 3;
    }
  }

  // Loading the reinforcement mesh
  Mesh reinforcement_mesh(dim, "reinforcement_mesh");

  // Exception is raised because reinforcement
  // mesh contains only segments, i.e. 1D elements
  try {
    reinforcement_mesh.read("reinforcement.msh");
  } catch (debug::Exception & e) {
  }

  // Model creation
  EmbeddedInterfaceModel model(mesh, reinforcement_mesh, dim);
  model.initFull(EmbeddedInterfaceModelOptions(_static, eigen_options())); // akantu-rs: Eigen

  // Boundary conditions
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "XBlocked");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "YBlocked");

  Vector<Real> force(dim);
  force(0) = 0.0;
  force(1) = -1.0;

  model.applyBC(BC::Neumann::FromTraction(force), "Force");

  // Dumping the concrete
  model.setBaseName("concrete");
  model.addDumpFieldVector("displacement");
  model.addDumpFieldVector("external_force");
  model.addDumpFieldVector("internal_force");
  model.addDumpFieldTensor("stress");

  // Dumping the reinforcement
  model.setBaseNameToDumper("reinforcement", "reinforcement");
  model.addDumpFieldTensorToDumper(
      "reinforcement", "stress_embedded"); // dumping stress in reinforcement

  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 1);
  solver.set("threshold", 1e-6);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);

  model.solveStep();

  // Dumping model
  model.dump();
  model.dump("reinforcement");

  finalize();
  return EXIT_SUCCESS;
}
