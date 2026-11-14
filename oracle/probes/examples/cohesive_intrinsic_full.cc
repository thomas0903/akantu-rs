// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/cohesive_intrinsic.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: Eigen's cache sizes are pinned to its defaults
// (decision D-050: host-independent blocking); `energy_full.csv` records,
// after every step, the number of cohesive elements and the dissipated,
// reversible, potential and kinetic energies with 17 significant digits.
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2012-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
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
#include "mesh_iterators.hh"
#include "solid_mechanics_model_cohesive.hh"
/* -------------------------------------------------------------------------- */
#include <fstream>  // akantu-rs
#include <iomanip>  // akantu-rs
#include <iostream>
/* -------------------------------------------------------------------------- */

using namespace akantu;

static void updateDisplacement(SolidMechanicsModelCohesive & model,
                               const ElementGroup & group, Real increment);

/* -------------------------------------------------------------------------- */
int main(int argc, char * argv[]) {
  // akantu-rs: D-050, Eigen's default cache sizes (host-independent blocking)
  Eigen::setCpuCacheSizes(16384, 524288, 524288); // akantu-rs
  initialize("material.dat", argc, argv);

  const Int spatial_dimension = 2;
  const Int max_steps = 350;

  Mesh mesh(spatial_dimension);
  mesh.read("triangle.msh");

  SolidMechanicsModelCohesive model(mesh);
  // To restric the insertion to the range [-0.26, -0.24] in the x direction
  model.getElementInserter().setLimit(_x, -0.26, -0.24);

  /// model initialization
  // _is_extrinsic = false for intrinsic
  model.initFull(_analysis_method = _explicit_lumped_mass,
                 _is_extrinsic = false);

  Real time_step = model.getStableTimeStep() * 0.8;
  model.setTimeStep(time_step);
  std::cout << "Time step: " << time_step << "\n";

  Array<bool> & boundary = model.getBlockedDOFs();

  /// boundary conditions
  boundary.set(true);

  model.setBaseName("intrinsic");
  model.addDumpFieldVector("displacement");
  model.addDumpField("velocity");
  model.addDumpField("acceleration");
  model.addDumpField("grad_u");
  model.addDumpField("external_force");
  model.addDumpField("internal_force");
  model.dump();

  /// update displacement
  auto && elements = mesh.createElementGroup("diplacement");
  Vector<Real> barycenter(spatial_dimension);

  for_each_element(
      mesh,
      [&](auto && el) {
        mesh.getBarycenter(el, barycenter);
        if (barycenter(_x) > -0.25) {
          elements.add(el, true);
        }
      },
      _element_kind = _ek_regular);

  Real increment = 0.01;

  updateDisplacement(model, elements, increment);

  std::ofstream energy_full("energy_full.csv"); // akantu-rs
  energy_full << std::setprecision(17)          // akantu-rs
              << "step,nb_cohesive,dissipated,reversible,potential,kinetic\n"; // akantu-rs
  /// Main loop
  for (Int s = 1; s <= max_steps; ++s) {
    model.solveStep();
    // akantu-rs: 17-digit energy record of the step
    energy_full << s << ","                                             // akantu-rs
                << mesh.getNbElement(spatial_dimension, _not_ghost, _ek_cohesive) << "," // akantu-rs
                << model.getEnergy("dissipated") << ","                    // akantu-rs
                << model.getEnergy("reversible") << ","                    // akantu-rs
                << model.getEnergy("potential") << ","                     // akantu-rs
                << model.getEnergy("kinetic") << "\n";                    // akantu-rs

    updateDisplacement(model, elements, increment);
    if (s % 1 == 0) {
      model.dump();
      std::cout << "passing step " << s << "/" << max_steps << "\n";
    }
  }

  Real Ed = model.getEnergy("dissipated");
  Real Edt = 2 * sqrt(2);

  std::cout << Ed << " " << Edt << "\n";

  if (Ed < Edt * 0.999 || Ed > Edt * 1.001 || std::isnan(Ed)) {
    std::cout << "The dissipated energy is incorrect"
              << "\n";
    return -1;
  }

  return 0;
}

/* -------------------------------------------------------------------------- */
static void updateDisplacement(SolidMechanicsModelCohesive & model,
                               const ElementGroup & group, Real increment) {
  Array<Real> & displacement = model.getDisplacement();

  for (auto && node : group.getNodeGroup().getNodes()) {
    displacement(node, 0) += increment;
  }
}
