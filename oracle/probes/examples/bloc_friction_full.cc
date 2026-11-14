// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/contact_mechanics_model/friction/bloc_friction.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: Eigen's cache sizes are
// pinned at the start of main (D-050); `energy_full.csv`: after every 30th
// step (the three loops numbered on) the number of contact elements, the
// potential and kinetic energies with 17 significant digits and the SHA-256
// of the raw little-endian bytes of displacement, velocity and gaps
// (concatenated in this order).
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
#include <fstream>
#include <iomanip>
#include "akantu_rs_sha256.hh" // akantu-rs
#include <vector>
/* -------------------------------------------------------------------------- */
using namespace akantu;
// akantu-rs: SHA-256 of the raw little-endian bytes of the arrays, concatenated
static std::string state_sha(std::initializer_list<const Array<Real> *> arrays) {
  akantu_rs::Sha256 h;
  for (const auto * a : arrays) {
    h.update(a->data(), sizeof(Real) * a->size() * a->getNbComponent());
  }
  return h.hex();
}
/* -------------------------------------------------------------------------- */

int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (not akantu_rs::Sha256::self_test()) { // akantu-rs
    std::cerr << "SHA-256 self-test failed\n"; // akantu-rs
    return 3;                                  // akantu-rs
  }                                            // akantu-rs

  // Initialize the material database
  initialize("material.dat", argc, argv);

  // Create the mesh>
  Mesh mesh(2); // Dimension 2

  // Read the mesh
  mesh.read("bloc.msh");

  // Create the model
  CouplerSolidContact coupler(mesh);

  // Initialize each model
  auto & solid = coupler.getSolidMechanicsModel();
  auto & contact = coupler.getContactMechanicsModel();
  auto && selector = std::make_shared<MeshDataMaterialSelector<std::string>>(
      "physical_names", solid);
  solid.setMaterialSelector(selector);

  // Initialize the coupler
  coupler.initFull(_analysis_method = _explicit_lumped_mass);
  Real time_step = solid.getStableTimeStep() * 0.1;
  coupler.setTimeStep(time_step);
  std::cout << "Time step: " << time_step << std::endl;

  // Setup the contact
  auto && surface_selector = std::make_shared<PhysicalSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);

  // Configuration of the dumper
  coupler.setBaseName("bloc_friction");
  coupler.addDumpFieldVector("displacement");
  coupler.addDumpFieldVector("velocity");
  coupler.addDumpFieldVector("normals");
  coupler.addDumpFieldVector("tangents");
  coupler.addDumpFieldVector("contact_force");
  coupler.addDumpFieldVector("external_force");
  coupler.addDumpFieldVector("internal_force");
  coupler.addDumpField("areas");
  coupler.addDumpField("stress");
  coupler.addDumpField("blocked_dofs");

  // Add the boundary conditions
  solid.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "XFixed");
  solid.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "YFixed");
  solid.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "loading");
  solid.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "loading");

  // Register velocity and gaps for future damping
  auto & velocity = solid.getVelocity();
  auto & gaps = contact.getGaps();

  std::ofstream energy_full("energy_full.csv");                                // akantu-rs
  energy_full << std::setprecision(17)                                          // akantu-rs
              << "step,nb_contact,potential,kinetic,sha256(displacement|velocity|gaps)\n"; // akantu-rs
  Int step = 0;                                                                 // akantu-rs
  // Dump the initial state
  coupler.dump();

  // First loop : compression of the bloc against the wall
  for (int s = 0; s < 10000; s++) {
    solid.applyBC(BC::Dirichlet::IncrementValue(-1.0 / 10000, _y), "loading");

    coupler.solveStep();

    // damping velocities only along the contacting zone
    for (auto && tuple : zip(gaps, make_view(velocity, 2))) {
      auto & gap = std::get<0>(tuple);
      auto & vel = std::get<1>(tuple);
      if (gap > 0) {
        vel *= 0.99;
      }
    }

    if (step % 30 == 0) // akantu-rs: every 30th step (1 MiB fixture budget)
      energy_full << step << "," << contact.getContactElements().size() << "," // akantu-rs
                << solid.getEnergy("potential") << ","                          // akantu-rs
                << solid.getEnergy("kinetic") << ","                            // akantu-rs
                << state_sha({&solid.getDisplacement(), &velocity, &gaps}) << "\n"; // akantu-rs
    ++step;                                                                   // akantu-rs
    if (s % 100 == 0) {
      std::cout << "Step " << s << "\t\r" << std::flush;
      coupler.dump();
    }
  }

  std::cout << "Compression done !" << std::endl;

  // Second loop : sliding of the bloc against the wall
  for (int s = 0; s < 10000; s++) {
    solid.applyBC(BC::Dirichlet::IncrementValue(2.0 / 10000, _x), "loading");

    coupler.solveStep();

    // damping velocities only along the contacting zone
    for (auto && tuple : zip(gaps, make_view(velocity, 2))) {
      auto & gap = std::get<0>(tuple);
      auto & vel = std::get<1>(tuple);
      if (gap > 0) {
        vel *= 0.99;
      }
    }

    if (step % 30 == 0) // akantu-rs: every 30th step (1 MiB fixture budget)
      energy_full << step << "," << contact.getContactElements().size() << "," // akantu-rs
                << solid.getEnergy("potential") << ","                          // akantu-rs
                << solid.getEnergy("kinetic") << ","                            // akantu-rs
                << state_sha({&solid.getDisplacement(), &velocity, &gaps}) << "\n"; // akantu-rs
    ++step;                                                                   // akantu-rs
    if (s % 100 == 0) {
      std::cout << "Step " << s << "\t\r" << std::flush;
      coupler.dump();
    }
  }

  std::cout << "Sliding done !" << std::endl;

  // Third loop : stabilization of the system (no external force)
  for (int s = 0; s < 10000; s++) {
    coupler.solveStep();

    // damping velocities only along the contacting zone
    for (auto && tuple : zip(gaps, make_view(velocity, 2))) {
      auto & gap = std::get<0>(tuple);
      auto & vel = std::get<1>(tuple);
      if (gap > 0) {
        vel *= 0.99;
      }
    }

    if (step % 30 == 0) // akantu-rs: every 30th step (1 MiB fixture budget)
      energy_full << step << "," << contact.getContactElements().size() << "," // akantu-rs
                << solid.getEnergy("potential") << ","                          // akantu-rs
                << solid.getEnergy("kinetic") << ","                            // akantu-rs
                << state_sha({&solid.getDisplacement(), &velocity, &gaps}) << "\n"; // akantu-rs
    ++step;                                                                   // akantu-rs
    if (s % 100 == 0) {
      std::cout << "Step " << s << "\t\r" << std::flush;
      coupler.dump();
    }
  }

  return 0;
}