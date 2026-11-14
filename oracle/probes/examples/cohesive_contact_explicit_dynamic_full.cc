// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/contact_mechanics_model/hertz/cohesive_contact_explicit_dynamic.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: Eigen's cache sizes are
// pinned at the start of main (D-050); `energy_full.csv`: after every 50th
// step (both loops, numbered on) the number of cohesive and contact elements, the
// potential and kinetic energies with 17 significant digits and the SHA-256
// of the raw little-endian bytes of displacement, velocity and gaps
// (concatenated in this order).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2021-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
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
#include "coupler_solid_cohesive_contact.hh"
#include <fstream>              // akantu-rs
#include <iomanip>              // akantu-rs
#include "akantu_rs_sha256.hh" // akantu-rs
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

  const Int spatial_dimension = 2;
  initialize("material-cohesive.dat", argc, argv);

  Real time_step{0.};
  Real time_factor{0.1};
  UInt max_steps{25000};
  Real max_displacement{1e-3};

  Mesh mesh(spatial_dimension);
  mesh.read("cohesive-contact.msh");

  CouplerSolidCohesiveContact coupler(mesh);

  auto & solid = coupler.getSolidMechanicsModelCohesive();
  auto & contact = coupler.getContactMechanicsModel();

  auto && material_selector =
      std::make_shared<MeshDataMaterialCohesiveSelector>(solid);
  material_selector->setFallback(solid.getMaterialSelector());
  solid.setMaterialSelector(material_selector);

  auto && surface_selector = std::make_shared<CohesiveSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);

  coupler.initFull(_analysis_method = _explicit_lumped_mass,
                   _is_extrinsic = true);

  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "sides");

  time_step = solid.getStableTimeStep();
  time_step *= time_factor;
  std::cout << "Time Step = " << time_step << "s (" << time_step << "s)"
            << "\n";
  coupler.setTimeStep(time_step);

  coupler.setBaseName("cohesive-contact-explicit-dynamic");
  coupler.addDumpFieldVector("displacement");
  coupler.addDumpFieldVector("velocity");
  coupler.addDumpFieldVector("normals");
  coupler.addDumpField("blocked_dofs");
  coupler.addDumpField("grad_u");
  coupler.addDumpField("stress");
  coupler.addDumpField("gaps");
  coupler.addDumpField("areas");

  auto & velocity = solid.getVelocity();
  auto & gaps = contact.getGaps();

  Real damping_ratio = 0.99;
  auto increment = max_displacement / max_steps;
  std::ofstream energy_full("energy_full.csv");                                // akantu-rs
  energy_full << std::setprecision(17)                                          // akantu-rs
              << "step,nb_cohesive,nb_contact,potential,kinetic,sha256(displacement|velocity|gaps)\n"; // akantu-rs
  Int step = 0;                                                                 // akantu-rs

  for (auto i : arange(max_steps)) {
    coupler.applyBC(BC::Dirichlet::IncrementValue(increment, _y), "loading");
    coupler.applyBC(BC::Dirichlet::IncrementValue(-increment, _y), "fixed");

    coupler.solveStep();

    solid.checkCohesiveStress();

    // damping velocities only along the contacting zone
    for (auto && tuple : zip(gaps, make_view(velocity, spatial_dimension))) {
      auto & gap = std::get<0>(tuple);
      auto & vel = std::get<1>(tuple);
      if (gap > 0) {
        vel *= damping_ratio;
      }
    }

    if (step % 50 == 0) // akantu-rs: every 50th step (1 MiB fixture budget)
      energy_full << step << ","                                              // akantu-rs
                << mesh.getNbElement(spatial_dimension, _not_ghost, _ek_cohesive) << "," // akantu-rs
                << contact.getContactElements().size() << ","                   // akantu-rs
                << solid.getEnergy("potential") << ","                          // akantu-rs
                << solid.getEnergy("kinetic") << ","                            // akantu-rs
                << state_sha({&solid.getDisplacement(), &velocity, &gaps}) << "\n"; // akantu-rs
    ++step;                                                                   // akantu-rs
    // dumping energies
    if (i % 1000 == 0) {
      Real epot = solid.getEnergy("potential");
      Real ekin = solid.getEnergy("kinetic");

      std::cerr << i << "," << i * increment << "," << epot << "," << ekin
                << "," << epot + ekin << ","
                << "\n";
    }

    if (i % 1000 == 0) {
      coupler.dump();
    }
  }

  for (auto i : arange(max_steps)) {
    solid.applyBC(BC::Dirichlet::IncrementValue(-increment, _y), "loading");
    solid.applyBC(BC::Dirichlet::IncrementValue(increment, _y), "fixed");

    coupler.solveStep();

    coupler.checkCohesiveStress();

    // damping velocities only along the contacting zone
    for (auto && tuple : zip(gaps, make_view(velocity, spatial_dimension))) {
      auto & gap = std::get<0>(tuple);
      auto & vel = std::get<1>(tuple);
      if (gap > 0) {
        vel *= damping_ratio;
      }
    }

    if (step % 50 == 0) // akantu-rs: every 50th step (1 MiB fixture budget)
      energy_full << step << ","                                              // akantu-rs
                << mesh.getNbElement(spatial_dimension, _not_ghost, _ek_cohesive) << "," // akantu-rs
                << contact.getContactElements().size() << ","                   // akantu-rs
                << solid.getEnergy("potential") << ","                          // akantu-rs
                << solid.getEnergy("kinetic") << ","                            // akantu-rs
                << state_sha({&solid.getDisplacement(), &velocity, &gaps}) << "\n"; // akantu-rs
    ++step;                                                                   // akantu-rs
    // dumping energies
    if (i % 1000 == 0) {
      Real epot = solid.getEnergy("potential");
      Real ekin = solid.getEnergy("kinetic");

      std::cerr << i << "," << i * increment << "," << epot << "," << ekin
                << "," << epot + ekin << ","
                << "\n";
    }

    if (i % 1000 == 0) {
      coupler.dump();
    }
  }
}
