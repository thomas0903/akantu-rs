// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_model/parallel/parallel_2d.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Change: `mesh.distribute()` is written out as its
// implementation (`Mesh::distributeImpl`: a `MeshPartitionScotch` of the mesh
// dimension partitioned on rank 0 with the default weights, then
// `distribute(partition)`) so that rank 0 can record the partition in
// `partition.txt` (linearised parts of the elements of the mesh dimension, one
// per line; replayed by the port, D-009).
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
#include "solid_mechanics_model.hh"
#include <fstream>                  // akantu-rs
#include <mesh_partition_scotch.hh> // akantu-rs
/* -------------------------------------------------------------------------- */

using namespace akantu;

int main(int argc, char * argv[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  initialize("material.dat", argc, argv);

  Int spatial_dimension = 2;
  Int max_steps = 10000;
  Real time_factor = 0.8;
  Real max_disp = 1e-6;

  Mesh mesh(spatial_dimension);

  const auto & comm = Communicator::getStaticCommunicator();
  Int prank = comm.whoAmI();
  if (prank == 0) {
    // Read the mesh
    mesh.read("square_2d.msh");
  }

  // akantu-rs: mesh.distribute() (Mesh::distributeImpl) with the partition
  // recorded on rank 0
  std::shared_ptr<MeshPartition> partition;
  if (comm.getNbProc() > 1 && prank == 0) {
    partition = std::make_shared<MeshPartitionScotch>(mesh, spatial_dimension);
    partition->partitionate(comm.getNbProc());
    std::ofstream parts("partition.txt");
    for (auto type :
         mesh.elementTypes(spatial_dimension, _not_ghost, _ek_not_defined)) {
      for (auto p : partition->getPartition(type, _not_ghost)) {
        parts << p << "\n";
      }
    }
  }
  mesh.distribute(partition);

  SolidMechanicsModel model(mesh);
  model.initFull();

  if (prank == 0) {
    std::cout << model.getMaterial(0) << "\n";
  }

  model.setBaseName("multi");
  model.addDumpFieldVector("displacement");
  model.addDumpFieldVector("velocity");
  model.addDumpFieldVector("acceleration");
  model.addDumpFieldTensor("stress");
  model.addDumpFieldTensor("grad_u");

  /// boundary conditions
  Real eps = 1e-16;
  const Array<Real> & pos = mesh.getNodes();
  Array<Real> & disp = model.getDisplacement();
  Array<bool> & boun = model.getBlockedDOFs();

  Real left_side = mesh.getLowerBounds()(0);
  Real right_side = mesh.getUpperBounds()(0);

  for (Int i = 0; i < mesh.getNbNodes(); ++i) {
    if (std::abs(pos(i, 0) - left_side) < eps) {
      disp(i, 0) = max_disp;
      boun(i, 0) = true;
    }

    if (std::abs(pos(i, 0) - right_side) < eps) {
      disp(i, 0) = -max_disp;
      boun(i, 0) = true;
    }
  }

  Real time_step = model.getStableTimeStep() * time_factor;
  std::cout << "Time Step = " << time_step << "s"
            << "\n";
  model.setTimeStep(time_step);

  model.dump();
  for (Int s = 1; s <= max_steps; ++s) {
    model.solveStep();
    if (s % 200 == 0) {
      model.dump();
    }

    if (prank == 0 && s % 100 == 0) {
      std::cout << "passing step " << s << "/" << max_steps << "\n";
    }
  }

  return 0;
}
