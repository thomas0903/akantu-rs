// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) (D-064), promoted from the review's probe rv84_ud632.cc:
// the current-position cache of SolidMechanicsModel across a second solver
// initialisation (src/model/solid_mechanics/solid_mechanics_model.cc:197-241
// initSolver, :411-430 updateCurrentPosition / getCurrentPosition, :448-453
// getStableTimeStep) on the setup of
// examples/c++/solid_mechanics_model/static/static.cc (frozen mesh
// square.msh, material.dat, the three Dirichlet groups), static solves with
// the Eigen sparse solver, Eigen's cache sizes pinned (D-050). Writes the
// fixture model/smm_init_solver/orderings, replayed by
// crates/akantu-solid/tests/smm_init_solver_ud352.rs.
//
// The nine call orderings (one fresh model each, static
// analysis, `initNewSolver(_explicit_lumped_mass)` as the second solver):
//   A          Dirichlet functors impose u, no solver step, reader;
//   D          A, then initNewSolver, reader;
//   B0         static solve, reader (refresh at the current release);
//   B1         B0, then initNewSolver, reader          (UD-352: X upstream);
//   C          static solve, no reader, initNewSolver, reader;
//   Dp         initFull, initNewSolver (u = 0), Dirichlet functors, reader;
//   E0         static solve, reader;
//   E1         E0, direct write (IncrementValue), initNewSolver, reader
//                                                       (UD-352: X upstream);
//   undeformed initFull, reader.
// Each recorded state is read in this order: getStableTimeStep(),
// getCurrentPosition(), getDisplacement(), the releases
// (getDisplacementRelease(), getCurrentPositionRelease()).
//
// Usage: smm_init_solver_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <mesh.hh>
#include <solid_mechanics_model.hh>

#include <Eigen/Core>

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <iomanip>
#include <iostream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

const char * const kProbe = "smm_init_solver_probe";
const char * const kProbeVersion = "1";

// D-050: Eigen's own default cache sizes (host-independent blocking).
constexpr std::ptrdiff_t kEigenL1 = 16384, kEigenL2 = 524288, kEigenL3 = 524288;

fs::path g_mesh;

std::vector<double> vec_of(const Array<Real> & a) {
  std::vector<double> v;
  v.reserve(a.size() * a.getNbComponent());
  for (Int i = 0; i < a.size(); ++i) {
    for (Int c = 0; c < a.getNbComponent(); ++c) {
      v.push_back(a(i, c));
    }
  }
  return v;
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

void init_static(SolidMechanicsModel & model) {
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
}

void apply_bcs(SolidMechanicsModel & model) {
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "Fixed_x");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "Fixed_y");
  model.applyBC(BC::Dirichlet::FixedValue(0.0001, _x), "Displacement");
}

void new_explicit_solver(SolidMechanicsModel & model) {
  model.initNewSolver(_explicit_lumped_mass);
}

void record(ap::FixtureWriter & w, const std::string & id, SolidMechanicsModel & model,
            const Array<Real> & X) {
  const Real dt = model.getStableTimeStep();
  const auto & cp = model.getCurrentPosition();
  const auto & u = model.getDisplacement();
  const auto nn = static_cast<std::size_t>(cp.size());
  const auto nc = static_cast<std::size_t>(cp.getNbComponent());
  w.add(id + "/dt", {1}, {"entry"}, "table", std::vector<double>{dt});
  w.add(id + "/current_position", {nn, nc}, {"node", "component"}, "nodal", vec_of(cp));
  w.add(id + "/displacement", {nn, nc}, {"node", "component"}, "nodal", vec_of(u));
  std::vector<std::int64_t> releases{model.getDisplacementRelease(),
                                     model.getCurrentPositionRelease()};
  w.add(id + "/releases", {2}, {"entry"}, "table", releases);
  // summary (as the review's rv84_ud632): sup-norm distances to X and X + u
  Real to_x = 0, to_xu = 0;
  for (Int i = 0; i < cp.size() * cp.getNbComponent(); ++i) {
    to_x = std::max(to_x, std::abs(cp.data()[i] - X.data()[i]));
    to_xu = std::max(to_xu, std::abs(cp.data()[i] - (X.data()[i] + u.data()[i])));
  }
  std::cout << id << " dt=" << dt << " |cp-X|=" << to_x << " |cp-(X+u)|=" << to_xu
            << " releases(u,cp)=" << releases[0] << "," << releases[1] << "\n";
}

} // namespace

int main(int argc, char ** argv) {
  if (argc != 4) {
    std::cerr << "usage: smm_init_solver_probe <fixtures_root> <upstream_src_root> "
                 "<frozen_meshes_root>\n";
    return 2;
  }
  // D-050: host-independent blocking of Eigen's dense and SparseLU kernels
  Eigen::setCpuCacheSizes(kEigenL1, kEigenL2, kEigenL3);
  const fs::path root = argv[1];
  const fs::path src = argv[2];
  const fs::path frozen = argv[3];
  initialize(argc, argv);
  std::cout << std::setprecision(17);
  const auto example = src / "examples/c++/solid_mechanics_model/static";
  const auto mat_file = example / "material.dat";
  g_mesh = frozen / "examples/c++/solid_mechanics_model/static/square.msh";
  getStaticParser().parse(mat_file.string());
  const Int dim = 2;

  ap::FixtureWriter w(root, "model/smm_init_solver/orderings",
                      "UD-352 (D-064): current position, stable time step, "
                      "displacement and releases after nine orderings of solve / reader / "
                      "initNewSolver / direct displacement writes (UD-352 in B1 and E1)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_tag("upstream_defect");
  w.set_notes("UD-352 (category B, D-064): in orderings B1 and E1 upstream's initSolver resets "
              "the release-cached current position to X after a reader refreshed it at the "
              "current displacement release, so getCurrentPosition() and getStableTimeStep() "
              "use the undeformed mesh; the port's corrected default keeps the cache (B1: X + "
              "u, E1: the last refresh X + u_old, the direct write being invisible, UD-800 C), "
              "its compat switch init_solver_resets_current_position reproduces every value. "
              "Orderings A, D, Dp: displacement imposed without a solver step stays invisible "
              "(UD-800, reproduced).");
  ap::Json c = ap::Json::array();
  c.push(static_cast<long long>(kEigenL1));
  c.push(static_cast<long long>(kEigenL2));
  c.push(static_cast<long long>(kEigenL3));
  w.parameters()["eigen_cache_sizes"] = c;
  w.parameters()["spatial_dimension"] = static_cast<long long>(dim);
  w.parameters()["sparse_solver"] = "eigen";
  w.parameters()["imposed_displacement"] = 0.0001;
  // the x86_64 oracle pin of the run (generate.sh): recorded as provenance
  ap::Json env = ap::Json::object();
  for (const char * var : {"OPENBLAS_CORETYPE", "OPENBLAS_NUM_THREADS"}) {
    const char * value = std::getenv(var);
    env[var] = std::string(value == nullptr ? "" : value);
  }
  w.parameters()["environment"] = env;
  ap::Json ids = ap::Json::array();
  for (const char * id : {"A", "D", "B0", "B1", "C", "Dp", "E0", "E1", "undeformed"}) {
    ids.push(std::string(id));
  }
  w.parameters()["orderings"] = ids;

  {
    Mesh mesh(dim);
    mesh.read(g_mesh.string());
    const auto nn = static_cast<std::size_t>(mesh.getNbNodes());
    w.add("mesh/nodes", {nn, static_cast<std::size_t>(dim)}, {"node", "component"}, "nodal",
          vec_of(mesh.getNodes()));
  }
  {
    Mesh mesh(dim);
    mesh.read(g_mesh.string());
    SolidMechanicsModel model(mesh);
    init_static(model);
    apply_bcs(model);
    const Array<Real> X(mesh.getNodes(), "X");
    record(w, "A", model, X);
    new_explicit_solver(model);
    record(w, "D", model, X);
  }
  {
    Mesh mesh(dim);
    mesh.read(g_mesh.string());
    SolidMechanicsModel model(mesh);
    init_static(model);
    apply_bcs(model);
    const Array<Real> X(mesh.getNodes(), "X");
    model.solveStep();
    record(w, "B0", model, X);
    new_explicit_solver(model);
    record(w, "B1", model, X);
  }
  {
    Mesh mesh(dim);
    mesh.read(g_mesh.string());
    SolidMechanicsModel model(mesh);
    init_static(model);
    apply_bcs(model);
    const Array<Real> X(mesh.getNodes(), "X");
    model.solveStep();
    new_explicit_solver(model);
    record(w, "C", model, X);
  }
  {
    Mesh mesh(dim);
    mesh.read(g_mesh.string());
    SolidMechanicsModel model(mesh);
    init_static(model);
    new_explicit_solver(model);
    apply_bcs(model);
    const Array<Real> X(mesh.getNodes(), "X");
    record(w, "Dp", model, X);
  }
  {
    Mesh mesh(dim);
    mesh.read(g_mesh.string());
    SolidMechanicsModel model(mesh);
    init_static(model);
    apply_bcs(model);
    const Array<Real> X(mesh.getNodes(), "X");
    model.solveStep();
    record(w, "E0", model, X);
    model.applyBC(BC::Dirichlet::IncrementValue(0.0001, _x), "Displacement");
    new_explicit_solver(model);
    record(w, "E1", model, X);
  }
  {
    Mesh mesh(dim);
    mesh.read(g_mesh.string());
    SolidMechanicsModel model(mesh);
    init_static(model);
    const Array<Real> X(mesh.getNodes(), "X");
    record(w, "undeformed", model, X);
  }
  w.add_input_file("mesh", g_mesh, "frozen mesh of examples/c++/solid_mechanics_model/static/square.geo (D-009)");
  w.add_input_file("material", mat_file, "upstream examples/c++/solid_mechanics_model/static/material.dat");
  w.finish();
  finalize();
  return 0;
}
