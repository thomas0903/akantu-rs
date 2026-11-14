// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_model/new_material/new_local_material.cc
// (reference 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except
// the lines marked `akantu-rs:`. Change: `energy_full.csv` records
// `s,epot,ekin` of every step with 17 significant digits.
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
#include "local_material_damage.hh"
#include "solid_mechanics_model.hh"
/* -------------------------------------------------------------------------- */
#include <fstream> // akantu-rs
#include <iomanip> // akantu-rs
#include <iostream>
/* -------------------------------------------------------------------------- */
using namespace akantu;

int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  akantu::initialize("material.dat", argc, argv);

  Int max_steps = 10000;
  Real epot{0.};
  Real ekin{0.};

  const Int dim = 2;

  Mesh mesh(dim);
  mesh.read("barre_trou.msh");

  /// model creation
  SolidMechanicsModel model(mesh);

  /// model initialization
  model.initFull(_analysis_method = _explicit_lumped_mass);

  std::cout << model.getMaterial(0) << "\n";

  Real time_step = model.getStableTimeStep();
  model.setTimeStep(time_step / 10.);

  /// Dirichlet boundary conditions
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "Fixed_x");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "Fixed_y");

  // Neumann boundary condition
  Matrix<Real> stress(dim, dim);
  stress.eye(3e2);
  model.applyBC(BC::Neumann::FromStress(stress), "Traction");

  model.setBaseName("local_material");
  model.addDumpField("displacement");
  model.addDumpField("velocity");
  model.addDumpField("acceleration");
  model.addDumpField("external_force");
  model.addDumpField("internal_force");
  model.addDumpField("grad_u");
  model.addDumpField("stress");
  model.addDumpField("damage");
  model.dump();

  std::ofstream energy_full("energy_full.csv");             // akantu-rs
  energy_full << std::setprecision(17) << "id,epot,ekin\n"; // akantu-rs
  for (Int s = 0; s < max_steps; ++s) {
    model.solveStep();

    epot = model.getEnergy("potential");
    ekin = model.getEnergy("kinetic");
    energy_full << s << "," << epot << "," << ekin << "\n"; // akantu-rs

    if (s % 100 == 0) {
      std::cout << s << " " << epot << " " << ekin << " " << epot + ekin
                << "\n";
    }

    if (s % 1000 == 0) {
      model.dump();
    }
  }

  return 0;
}
