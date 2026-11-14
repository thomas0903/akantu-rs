// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_solver/test_sparse_solver_mumps.cc
// at 35a851780d0b9471d258ed53a003ae98aaa95ea0 (mesh, DOF manager, residual
// assembly, SparseSolverMumps::solve). akantu-rs oracle probe:
// instead of the test's diagonal, every rank adds the rank-partial AIJ
// entries exported by the Rust acceptance binary
// (crates/akantu-linalg/tests/mumps_mpi.rs, AKANTU_MUMPS_EXPORT) to the DOF
// manager's matrix with SparseMatrixAIJ::add(i, j, v) in file order — which
// rebuilds the same irn/jcn/a arrays — and the global right-hand side
// b(g) through the local-or-master DOFs; then upstream SparseSolverMumps
// solves (ICNTL(18) = 3, centralised RHS). Every rank writes its
// local-or-master solution entries (g, x) for the comparison script.
// Upstream copyright notice preserved below.
/**
 * Copyright (©) 2017-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
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
#include "aka_common.hh"
#include "dof_manager_default.hh"
#include "dof_synchronizer.hh"
#include "mesh.hh"
#include "mesh_accessor.hh"
#include "sparse_matrix_aij.hh"
#include "sparse_solver_mumps.hh"
/* -------------------------------------------------------------------------- */
#include <cstdio>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>
/* -------------------------------------------------------------------------- */
using namespace akantu;

static void genMesh(Mesh & mesh, Int nb_nodes) {
  MeshAccessor mesh_accessor(mesh);
  Array<Real> & nodes = mesh_accessor.getNodes();
  Array<Idx> & conn = mesh_accessor.getConnectivity(_segment_2);
  nodes.resize(nb_nodes);
  for (Int n = 0; n < nb_nodes; ++n) {
    nodes(n, _x) = n * (1. / (nb_nodes - 1));
  }
  conn.resize(nb_nodes - 1);
  for (Int n = 0; n < nb_nodes - 1; ++n) {
    conn(n, 0) = n;
    conn(n, 1) = n + 1;
  }
  mesh_accessor.makeReady();
}

int main(int argc, char * argv[]) {
  initialize(argc, argv);
  if (argc < 5) {
    std::cerr << "usage: mumps_probe <in-dir> <out-dir> <tag> <system>..." << std::endl;
    return 2;
  }
  std::string in = argv[1], out = argv[2], tag = argv[3];
  const auto & comm = Communicator::getStaticCommunicator();
  Int psize = comm.getNbProc();
  Int prank = comm.whoAmI();
  for (int s = 4; s < argc; ++s) {
    std::string name = argv[s];
    std::string base = in + "/" + name + ".p" + std::to_string(psize);
    std::ifstream fa(base + ".r" + std::to_string(prank) + ".txt");
    std::ifstream fb(base + ".rhs.txt");
    if (!fa || !fb) {
      std::cerr << "missing input for " << base << std::endl;
      return 3;
    }
    Int n, sym, nnz;
    fa >> n >> sym >> nnz;
    std::vector<Idx> ii(nnz), jj(nnz);
    std::vector<Real> vv(nnz);
    for (Int k = 0; k < nnz; ++k) {
      std::string v;
      fa >> ii[k] >> jj[k] >> v;
      vv[k] = std::strtod(v.c_str(), nullptr);
    }
    Int nb;
    fb >> nb;
    std::vector<Real> rhs(nb);
    for (Int k = 0; k < nb; ++k) {
      std::string v;
      fb >> v;
      rhs[k] = std::strtod(v.c_str(), nullptr);
    }

    Mesh mesh(1);
    if (prank == 0) {
      genMesh(mesh, n);
    }
    mesh.distribute();
    Int nb_nodes = mesh.getNbNodes();
    DOFManagerDefault dof_manager(mesh, "probe_dof_manager");
    Array<Real> x(nb_nodes);
    x.set(0.);
    dof_manager.registerDOFs("x", x, _dst_nodal);
    if (dof_manager.getSystemSize() != n) {
      std::cerr << "system size " << dof_manager.getSystemSize() << " != " << n << std::endl;
      return 4;
    }
    const auto & local_equation_number = dof_manager.getLocalEquationsNumbers("x");
    auto & A = aka::as_type<SparseMatrixAIJ>(
        dof_manager.getNewMatrix("A", sym ? _symmetric : _unsymmetric));
    for (Int k = 0; k < nnz; ++k) {
      A.add(ii[k], jj[k], vv[k]);
    }
    if (A.getNbNonZero() != nnz) {
      std::cerr << "rebuilt nnz " << A.getNbNonZero() << " != " << nnz << std::endl;
      return 5;
    }
    Array<Real> b(nb_nodes);
    b.set(0.);
    for (Int i = 0; i < nb_nodes; ++i) {
      if (dof_manager.isLocalOrMasterDOF(i)) {
        auto gi = dof_manager.localToGlobalEquationNumber(local_equation_number(i));
        b(i) = rhs[gi];
      }
    }
    dof_manager.assembleToResidual("x", b);
    SparseSolverMumps solver(dof_manager, "A");
    solver.solve();
    Array<Real> & y = dof_manager.getSolution("x");
    std::string fo = out + "/" + name + ".p" + std::to_string(psize) + "." + tag + ".r" +
                     std::to_string(prank) + ".sol";
    FILE * f = std::fopen(fo.c_str(), "w");
    for (Int i = 0; i < nb_nodes; ++i) {
      if (dof_manager.isLocalOrMasterDOF(i)) {
        auto gi = dof_manager.localToGlobalEquationNumber(local_equation_number(i));
        std::fprintf(f, "%lld %.17g\n", static_cast<long long>(gi), y(i));
      }
    }
    std::fclose(f);
    if (prank == 0) {
      std::cout << "solved " << name << " n=" << n << " ranks=" << psize << std::endl;
    }
  }
  finalize();
  return 0;
}
