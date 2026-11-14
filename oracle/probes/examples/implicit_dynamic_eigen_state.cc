// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_model/implicit/implicit_dynamic.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`: oracle/probes/examples/implicit_dynamic_eigen.cc (Eigen
// sparse solver pinned, `position_full.csv` with 17 significant digits,
// Eigen's cache sizes pinned at the start of main, D-050) plus
// `state_sha256.txt`: for every step `s`, the SHA-256 of the raw
// little-endian bytes of displacement, velocity and acceleration (nodal
// arrays, node-major, in this order, concatenated) after `solveStep`
// (the full state of every step, not one scalar).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2014-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
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
#include <fstream>
#include <iomanip> // akantu-rs
#include "akantu_rs_sha256.hh" // akantu-rs
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

const Real bar_length = 10.;
const Real bar_height = 1.;
const Real bar_depth = 1.;
const Real F = 5e3;
const Real L = bar_length;
const Real I = bar_depth * bar_height * bar_height * bar_height / 12.;
const Real E = 12e7;
const Real rho = 1000;
const Real m = rho * bar_height * bar_depth;

static Real w(UInt n) {
  return n * n * M_PI * M_PI / (L * L) * sqrt(E * I / m);
}

static Real analytical_solution(Real time) {
  return 2 * F * L * L * L / (pow(M_PI, 4) * E * I) *
         ((1. - cos(w(1) * time)) + (1. - cos(w(3) * time)) / 81. +
          (1. - cos(w(5) * time)) / 625.);
}

const Int spatial_dimension = 2;
const Real time_step = 1e-4;
const Real max_time = 0.62;
/* -------------------------------------------------------------------------- */
int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (not akantu_rs::Sha256::self_test()) { // akantu-rs
    std::cerr << "akantu-rs: SHA-256 self-test failed\n"; // akantu-rs
    return 3;                                             // akantu-rs
  }                                                       // akantu-rs

  initialize("material_dynamic.dat", argc, argv);

  Mesh mesh(spatial_dimension);

  const auto & comm = Communicator::getStaticCommunicator();
  Int prank = comm.whoAmI();

  if (prank == 0) {
    mesh.read("beam.msh");
  }

  mesh.distribute();

  SolidMechanicsModel model(mesh);

  /// model initialization
  model.initFull(_analysis_method = _implicit_dynamic,
                 _solver_options = eigen_options()); // akantu-rs: Eigen
  Material & mat = model.getMaterial(0);
  mat.set("E", E);
  mat.set("rho", rho);

  Array<Real> & force = model.getExternalForce();
  Array<Real> & displacment = model.getDisplacement();

  // boundary conditions
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "blocked");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "blocked");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "roller");

  const Array<Idx> & trac_nodes =
      mesh.getElementGroup("traction").getNodeGroup().getNodes();

  bool dump_node = false;
  if (not trac_nodes.empty() and mesh.isLocalOrMasterNode(trac_nodes(0))) {
    force(trac_nodes(0), 1) = F;
    dump_node = true;
  }

  // output setup
  std::ofstream pos;
  pos.open("position.csv");
  if (not pos.good()) {
    AKANTU_ERROR("Cannot open file \"position.csv\"");
  }

  pos << "id,time,position,solution"
      << "\n";
  std::ofstream pos_full("position_full.csv");                       // akantu-rs
  pos_full << std::setprecision(17) << "id,time,position,solution\n"; // akantu-rs
  std::ofstream state("state_sha256.txt");                           // akantu-rs
  state << "step\tsha256(displacement|velocity|acceleration)\n";     // akantu-rs

  model.setBaseName("dynamic");
  model.addDumpFieldVector("displacement");
  model.addDumpField("velocity");
  model.addDumpField("acceleration");
  model.addDumpField("external_force");
  model.addDumpField("internal_force");
  model.dump();

  model.setTimeStep(time_step);

  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 100);
  solver.set("threshold", 1e-12);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);

  /// time loop
  Real time = 0.;
  for (Int s = 1; time < max_time; ++s, time += time_step) {
    if (prank == 0) {
      std::cout << s << "\r" << std::flush;
    }

    model.solveStep();

    if (dump_node) {
      pos << s << "," << time << "," << displacment(trac_nodes(0), 1) << ","
          << analytical_solution(s * time_step) << "\n";
      pos_full << s << "," << time << "," << displacment(trac_nodes(0), 1) // akantu-rs
               << "," << analytical_solution(s * time_step) << "\n";      // akantu-rs
    }

    { // akantu-rs: digest of the full state after the step
      akantu_rs::Sha256 h;
      for (const Array<Real> * a :
           {&model.getDisplacement(), &model.getVelocity(),
            &model.getAcceleration()}) {
        h.update(a->data(), a->size() * a->getNbComponent() * sizeof(Real));
      }
      state << s << "\t" << h.hex() << "\n";
    } // akantu-rs

    if (s % 100 == 0) {
      model.dump();
    }
  }

  std::cout << "\n";
  pos.close();
  state.close(); // akantu-rs

  return 0;
}
