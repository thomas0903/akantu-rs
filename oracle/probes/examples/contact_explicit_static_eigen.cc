// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/contact_mechanics_model/hertz/contact_explicit_static.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: the Eigen sparse solver is
// pinned; Eigen's cache sizes are pinned at the start of main (D-050);
// `newton_full.csv`: after every step the number of contact elements and the
// Newton iteration count and error (17 significant digits).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2013-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
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
#include "coupler_solid_contact.hh"
#include "non_linear_solver.hh"
#include <fstream>  // akantu-rs
#include <iomanip>  // akantu-rs
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
/* -------------------------------------------------------------------------- */

int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);

  const Int spatial_dimension = 2;
  initialize("material.dat", argc, argv);

  Mesh mesh(spatial_dimension);
  mesh.read("hertz.msh");

  CouplerSolidContact coupler(mesh);

  auto & solid = coupler.getSolidMechanicsModel();
  auto & contact = coupler.getContactMechanicsModel();

  auto && selector = std::make_shared<MeshDataMaterialSelector<std::string>>(
      "physical_names", solid);
  solid.setMaterialSelector(selector);

  coupler.initFull(_analysis_method = _static,
                   _solver_options = eigen_options()); // akantu-rs: Eigen

  auto && surface_selector = std::make_shared<PhysicalSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);

  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "fixed");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "fixed");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "loading");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "symmetry");

  coupler.setBaseName("contact-explicit-static");
  coupler.addDumpFieldVector("displacement");
  coupler.addDumpFieldVector("normals");
  coupler.addDumpFieldVector("contact_force");
  coupler.addDumpFieldVector("external_force");
  coupler.addDumpFieldVector("internal_force");
  coupler.addDumpField("gaps");
  coupler.addDumpField("areas");
  coupler.addDumpField("blocked_dofs");
  coupler.addDumpField("grad_u");
  coupler.addDumpField("stress");

  auto max_steps = 100U;
  std::ofstream newton_full("newton_full.csv");            // akantu-rs
  newton_full << std::setprecision(17)                      // akantu-rs
              << "step,nb_contact,nb_iterations,error\n"; // akantu-rs
  Int step = 0;                                              // akantu-rs

  for (auto _ [[gnu::unused]] : arange(max_steps)) {
    auto increment = 1e-4;
    coupler.applyBC(BC::Dirichlet::IncrementValue(-increment, _y), "loading");

    coupler.solveStep();
    {                                                                  // akantu-rs
      auto & nls = solid.getNonLinearSolver();                         // akantu-rs
      Int nb_iter = nls.get("nb_iterations");                          // akantu-rs
      Real error = nls.get("error");                                   // akantu-rs
      newton_full << step++ << "," << contact.getContactElements().size() // akantu-rs
                  << "," << nb_iter << "," << error << "\n";          // akantu-rs
    }                                                                  // akantu-rs
    coupler.dump();
  }

  return 0;
}
