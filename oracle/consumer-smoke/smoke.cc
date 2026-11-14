// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/static/static.cc
// Out-of-tree consumer smoke test of an oracle install tree: static linear
// elastic solve (default implicit solver), prints a displacement checksum.
#include "non_linear_solver.hh"
#include "solid_mechanics_model.hh"

#include <cmath>
#include <cstdio>

using namespace akantu;

int main(int argc, char * argv[]) {
  initialize("material.dat", argc, argv);

  const Int spatial_dimension = 2;
  Mesh mesh(spatial_dimension);
  mesh.read("square.msh");

  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static);

  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "Fixed_x");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "Fixed_y");
  model.applyBC(BC::Dirichlet::FixedValue(0.0001, _x), "Displacement");

  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 2);
  solver.set("threshold", 2e-4);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);
  model.solveStep();

  const auto & u = model.getDisplacement();
  Real sum = 0., sum_sq = 0.;
  for (Idx n = 0; n < u.size(); ++n) {
    for (Int d = 0; d < spatial_dimension; ++d) {
      sum += u(n, d);
      sum_sq += u(n, d) * u(n, d);
    }
  }
  std::printf("nodes=%d sum_u=%.17g norm_u=%.17g\n", int(u.size()), sum,
              std::sqrt(sum_sq));
  finalize();
  return 0;
}
