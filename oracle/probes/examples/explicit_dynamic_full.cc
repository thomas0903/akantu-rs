// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Change: `energy_full.csv` repeats `energy.csv` with 17
// significant digits.
// Eigen's cache sizes are pinned at the start of main (D-050).
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
#include <fstream>
#include <iomanip> // akantu-rs
#include <solid_mechanics_model.hh>
/* -------------------------------------------------------------------------- */

using namespace akantu;

int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  initialize("material.dat", argc, argv);

  const Int spatial_dimension = 3;
  const Real pulse_width = 2.;
  const Real A = 0.01;
  Real time_step;
  Real time_factor = 0.8;
  Int max_steps = 1000;
  Mesh mesh(spatial_dimension);

  if (Communicator::getStaticCommunicator().whoAmI() == 0)
    mesh.read("bar.msh");

  SolidMechanicsModel model(mesh);

  /// model initialization
  model.initFull(_analysis_method = _explicit_lumped_mass);

  time_step = model.getStableTimeStep();
  std::cout << "Time Step = " << time_step * time_factor << "s (" << time_step
            << "s)" << std::endl;
  time_step *= time_factor;
  model.setTimeStep(time_step);

  /// boundary and initial conditions
  Array<Real> & displacement = model.getDisplacement();
  const Array<Real> & nodes = mesh.getNodes();

  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    Real x = nodes(n) - 2;
    // Sinus * Gaussian
    Real L = pulse_width;
    Real k = 0.1 * 2 * M_PI * 3 / L;
    displacement(n) = A * sin(k * x) * exp(-(k * x) * (k * x) / (L * L));
  }

  std::ofstream energy;
  energy.open("energy.csv");

  energy << "id,rtime,epot,ekin,tot" << std::endl;
  std::ofstream energy_full("energy_full.csv");                      // akantu-rs
  energy_full << std::setprecision(17) << "id,rtime,epot,ekin,tot\n"; // akantu-rs

  model.setBaseName("explicit_dynamic");
  model.addDumpField("displacement");
  model.addDumpField("velocity");
  model.addDumpField("acceleration");
  model.addDumpField("stress");
  model.dump();

  for (Int s = 1; s <= max_steps; ++s) {
    model.solveStep();

    Real epot = model.getEnergy("potential");
    Real ekin = model.getEnergy("kinetic");

    energy << s << "," << s * time_step << "," << epot << "," << ekin << ","
           << epot + ekin << "," << std::endl;
    energy_full << s << "," << s * time_step << "," << epot << "," << ekin // akantu-rs
                << "," << epot + ekin << ",\n";                          // akantu-rs

    if (s % 10 == 0)
      std::cout << "passing step " << s << "/" << max_steps << std::endl;
    model.dump();
  }

  energy.close();
}
