// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/phase_field_model/phase_field_parallel.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: Eigen's cache sizes are pinned at the start of
// main (D-050); `mesh.distribute()` is written out as its implementation
// (`Mesh::distributeImpl`: a `MeshPartitionScotch` of the mesh dimension
// partitioned on rank 0 with the default weights, then
// `distribute(partition)`) so that rank 0 records the partition in
// `partition.txt` (as oracle/probes/examples/parallel_2d_partition.cc);
// `state_sha256.proc<rank>.txt`: after every load step, the SHA-256 of the
// raw little-endian bytes of the rank's displacement and damage (local nodal
// arrays, concatenated in this order).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * @file   phase_field_parallel.cc
 *
 * @author Mohit Pundir <mohit.pundir@ethz.ch>
 *
 * @date creation: Mon May 09 2022
 * @date last modification: Mon May 09 2022
 *
 * @brief  Example of phase field model in parallel
 *
 *
 * @section LICENSE
 *
 * Copyright (©) 2018-2021 EPFL (Ecole Polytechnique Fédérale de Lausanne)
 * Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
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
 *
 */

/* -------------------------------------------------------------------------- */
#include "coupler_solid_phasefield.hh"
/* -------------------------------------------------------------------------- */
#include <chrono>
#include <fstream>
#include <mesh_partition_scotch.hh> // akantu-rs
#include "akantu_rs_sha256.hh"       // akantu-rs
#include <iostream>
/* -------------------------------------------------------------------------- */

using namespace akantu;
using clk = std::chrono::high_resolution_clock;
using second = std::chrono::duration<double>;
using millisecond = std::chrono::duration<double, std::milli>;

const UInt spatial_dimension = 2;

// akantu-rs: SHA-256 of the raw little-endian bytes of the arrays, concatenated
static std::string state_sha(std::initializer_list<const Array<Real> *> arrays) {
  akantu_rs::Sha256 h;
  for (const auto * a : arrays) {
    h.update(a->data(), sizeof(Real) * a->size() * a->getNbComponent());
  }
  return h.hex();
}

int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);

  initialize("material_notch.dat", argc, argv);

  // create mesh
  Mesh mesh(spatial_dimension);

  const auto & comm = Communicator::getStaticCommunicator();
  Int prank = comm.whoAmI();
  if (prank == 0) {
    // Read the mesh
    mesh.read("square_notch.msh");
  }

  // akantu-rs: mesh.distribute() (Mesh::distributeImpl) with the partition
  // recorded on rank 0
  std::shared_ptr<MeshPartition> partition;                              // akantu-rs
  if (comm.getNbProc() > 1 && prank == 0) {                              // akantu-rs
    partition = std::make_shared<MeshPartitionScotch>(mesh, spatial_dimension); // akantu-rs
    partition->partitionate(comm.getNbProc());                           // akantu-rs
    std::ofstream parts("partition.txt");                                // akantu-rs
    for (auto type :                                                     // akantu-rs
         mesh.elementTypes(spatial_dimension, _not_ghost, _ek_not_defined)) { // akantu-rs
      for (auto p : partition->getPartition(type, _not_ghost)) {         // akantu-rs
        parts << p << "\n";                                              // akantu-rs
      }                                                                  // akantu-rs
    }                                                                    // akantu-rs
  }                                                                      // akantu-rs
  mesh.distribute(partition);                                            // akantu-rs

  CouplerSolidPhaseField coupler(mesh);
  auto & model = coupler.getSolidMechanicsModel();
  auto & phase = coupler.getPhaseFieldModel();

  model.initFull(_analysis_method = _static);
  auto && mat_selector =
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names",
                                                              model);
  model.setMaterialSelector(mat_selector);

  auto && selector = std::make_shared<MeshDataPhaseFieldSelector<std::string>>(
      "physical_names", phase);
  phase.setPhaseFieldSelector(selector);

  phase.initFull(_analysis_method = _static);

  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "bottom");
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");

  model.setBaseName("phase_notch_parallel");
  model.addDumpField("stress");
  model.addDumpField("grad_u");
  model.addDumpFieldVector("displacement");
  model.addDumpField("damage");
  if (mesh.isDistributed()) {
    phase.addDumpField("partitions");
  }
  phase.dump();

  UInt nbSteps = 1000;
  Real increment = 4e-6;
  UInt nb_staggered_steps = 10;

  Int nbNodes = mesh.getNbNodes();
  std::ofstream state("state_sha256.proc" + std::to_string(prank) + ".txt"); // akantu-rs
  state << "step\tsha256(displacement|damage)\n";                      // akantu-rs
  std::cout << "Number of nodes: " << nbNodes << std::endl;

  auto start_time = clk::now();

  for (UInt s = 1; s < nbSteps; ++s) {

    if (s % 10 == 0 && prank == 0) {
      constexpr char wheel[] = "/-\\|";
      auto elapsed = clk::now() - start_time;
      auto time_per_step = elapsed / s;
      std::cout << "\r[" << wheel[(s / 10) % 4] << "] " << std::setw(5) << s
                << "/" << nbSteps << " (" << std::setprecision(2) << std::fixed
                << std::setw(8) << millisecond(time_per_step).count()
                << "ms/step - elapsed: " << std::setw(8)
                << second(elapsed).count() << "s - ETA: " << std::setw(8)
                << second((nbSteps - s) * time_per_step).count() << "s)"
                << std::string(' ', 20) << std::flush;
    }
    model.applyBC(BC::Dirichlet::IncrementValue(increment, _y), "top");

    for (UInt i = 0; i < nb_staggered_steps; ++i) {
      coupler.solve();
    }

    state << s << "\t"                                                  // akantu-rs
          << state_sha({&model.getDisplacement(), &phase.getDamage()}) << "\n"; // akantu-rs
    if (s % 100 == 0) {
      model.dump();
    }
  }

  return 0;
}
