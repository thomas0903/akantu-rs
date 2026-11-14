// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/structural_mechanics_model/bernoulli_beam_2_example.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: `initFull(_analysis_method = _static)` with
// the Eigen sparse solver pinned (UD-514: upstream's `initFull()` uses the
// default `_explicit_lumped_mass`, so its `solveStep` is one explicit step
// instead of the static solution the example prints; the port runs `_static`
// by default, D-003 B); Eigen's cache sizes are pinned at the start of main
// (D-050);
// `values_full.txt` holds the five printed values with 17 significant digits
// and the full displacement, stress (N, M per quadrature point) and internal
// force arrays (one value per line).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2011-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
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
#include "mesh_accessor.hh"
#include "structural_mechanics_model.hh"
/* -------------------------------------------------------------------------- */
#include <fstream>  // akantu-rs
#include <iomanip>  // akantu-rs
#include <iostream>
/* -------------------------------------------------------------------------- */

#define TYPE _bernoulli_beam_2

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
  initialize(argc, argv);
  // Defining the mesh
  Mesh beams(2);

  const auto q = 6000.;
  const auto L = 10.;
  const auto M = -3600.; // Momentum at 3

  auto nb_nodes = 3;
  auto nb_element = nb_nodes - 1;

  MeshAccessor mesh_accessor(beams);
  Array<Real> & nodes = mesh_accessor.getNodes();
  nodes.resize(nb_nodes);

  beams.addConnectivityType(_bernoulli_beam_2);
  auto & connectivity = mesh_accessor.getConnectivity(_bernoulli_beam_2);
  connectivity.resize(nb_element);

  nodes.zero();
  nodes(1, 0) = 10;
  nodes(2, 0) = 18;

  for (int i = 0; i < nb_element; ++i) {
    connectivity(i, 0) = i;
    connectivity(i, 1) = i + 1;
  }

  mesh_accessor.makeReady();

  // Defining the materials
  StructuralMechanicsModel model(beams);

  StructuralMaterial mat1;
  mat1.E = 3e10;
  mat1.I = 0.0025;
  mat1.A = 0.01;

  model.addMaterial(mat1);

  StructuralMaterial mat2;
  mat2.E = 3e10;
  mat2.I = 0.00128;
  mat2.A = 0.01;

  model.addMaterial(mat2);

  // Defining the forces
  model.initFull(_analysis_method = _static,
                 _solver_options = eigen_options()); // akantu-rs: Eigen

  auto & forces = model.getExternalForce();
  auto & displacement = model.getDisplacement();
  auto & boundary = model.getBlockedDOFs();
  const auto & N_M = model.getStress(_bernoulli_beam_2);

  auto & element_material = model.getElementMaterial(_bernoulli_beam_2);

  boundary.set(false);
  forces.zero();
  displacement.zero();

  element_material(1) = 1;

  forces(0, 1) = -q * L / 2.;
  forces(0, 2) = -q * L * L / 12.;
  forces(1, 1) = -q * L / 2.;
  forces(1, 2) = q * L * L / 12.;
  forces(2, 2) = M;
  forces(2, 0) = mat2.E * mat2.A / 18;

  // Defining the boundary conditions
  boundary(0, 0) = true;
  boundary(0, 1) = true;
  boundary(0, 2) = true;
  boundary(1, 1) = true;
  boundary(2, 1) = true;

  model.addDumpFieldVector("displacement");
  model.addDumpField("rotation");
  model.addDumpFieldVector("force");
  model.addDumpField("momentum");

  model.solveStep();

  model.assembleResidual();

  // Post-Processing
  std::cout << " d1 = " << displacement(1, 2) << std::endl;
  std::cout << " d2 = " << displacement(2, 2) << std::endl;
  std::cout << " d3 = " << displacement(1, 0) << std::endl;
  std::cout << " M1 = " << N_M(0, 1) << std::endl;
  std::cout << " M2 = " << N_M(2 * (nb_nodes - 2), 1) << std::endl;

  model.dump();

  // akantu-rs: 17-digit record of the printed values and of the state
  {                                                                    // akantu-rs
    std::ofstream full("values_full.txt");                             // akantu-rs
    full << std::setprecision(17);                                     // akantu-rs
    full << "d1 " << displacement(1, 2) << "\n"                        // akantu-rs
         << "d2 " << displacement(2, 2) << "\n"                        // akantu-rs
         << "d3 " << displacement(1, 0) << "\n"                        // akantu-rs
         << "M1 " << N_M(0, 1) << "\n"                                 // akantu-rs
         << "M2 " << N_M(2 * (nb_nodes - 2), 1) << "\n";               // akantu-rs
    auto all = [&full](const char * name, const Array<Real> & a) {     // akantu-rs
      for (Int i = 0; i < a.size() * a.getNbComponent(); ++i) {        // akantu-rs
        full << name << " " << a.data()[i] << "\n";                    // akantu-rs
      }                                                                // akantu-rs
    };                                                                 // akantu-rs
    all("displacement", displacement);                                 // akantu-rs
    all("stress", N_M);                                                // akantu-rs
    all("internal_force", model.getInternalForce());                   // akantu-rs
  }                                                                    // akantu-rs

  finalize();
}
