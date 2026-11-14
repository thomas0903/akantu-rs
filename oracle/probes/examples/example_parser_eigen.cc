// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_model/io/parser/example_parser.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: the Eigen sparse solver is pinned;
// `external_force.csv` (17 digits) and `newton.txt` record the load and the
// Newton history.
// Eigen's cache sizes are pinned at the start of main (D-050).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
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
#include "non_linear_solver.hh"
#include <fstream> // akantu-rs
#include <iomanip> // akantu-rs
#include "solid_mechanics_model.hh"
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

  // Precise in initialize the name of the text input file to parse.
  initialize("input_file.dat", argc, argv);

  // Get the user ParserSection.
  const ParserSection & usersect = getUserParser();

  // getParameterValue() allows to extract data associated to a given parameter
  // name
  // and cast it in the desired type set as template paramter.
  Mesh mesh(usersect.getParameterValue<UInt>("spatial_dimension"));
  mesh.read(usersect.getParameterValue<std::string>("mesh_file"));

  // getParameter() can be used with variable declaration (destination type is
  // explicitly known).
  Int max_iter = usersect.getParameter("max_nb_iterations");
  Real precision = usersect.getParameter("precision");

  // Following NumPy convention, data can be interpreted as Vector or Matrix
  // structures.
  Matrix<Real> eigen_stress = usersect.getParameter("stress");

  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static,
                 _solver_options = eigen_options()); // akantu-rs: Eigen

  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x),
                usersect.getParameterValue<std::string>("outter_crust"));
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y),
                usersect.getParameterValue<std::string>("outter_crust"));
  model.applyBC(BC::Neumann::FromStress(eigen_stress),
                usersect.getParameterValue<std::string>("inner_holes"));

  model.setBaseName("swiss_cheese");
  model.addDumpFieldVector("displacement");

  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", max_iter);
  solver.set("threshold", precision);

  // akantu-rs: the assembled load at 17 digits
  {
    std::ofstream f_ext("external_force.csv");
    f_ext << std::setprecision(17);
    const Array<Real> & force = model.getExternalForce();
    for (Int n = 0; n < force.size(); ++n) {
      const Real fx = force(n, 0);
      const Real fy = force(n, 1);
      f_ext << fx << "," << fy << "\n";
    }
  }

  model.solveStep();

  // akantu-rs: Newton history
  {
    Int nb_iterations = solver.get("nb_iterations");
    Real error = solver.get("error");
    std::ofstream newton("newton.txt");
    newton << std::setprecision(17) << "nb_iterations " << nb_iterations
           << "\nerror " << error << "\n";
  }

  model.dump();

  return 0;
}
