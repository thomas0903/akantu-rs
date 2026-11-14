// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): replays upstream test programs of the
// solid-mechanics / material families and records what they compute or
// print, for the Rust ports crates/akantu-solid/tests/upstream_*.rs.
// Derived from test/test_model/test_solid_mechanics_model/test_materials/
// {test_material_mazars.cc, test_material_fixtures.hh,
// test_material_elasto_plastic_linear_isotropic_hardening/
// test_material_elasto_plastic_linear_isotropic_hardening.cc}.
//
// Cases (case ids model/upstream_tests_2b/<case>, format v1):
//   material_mazars_hexahedron_8
//       test_material_mazars.cc verbatim except the connectivity reference
//       (`Array<Idx> &`: upstream's `Array<UInt> &` does not compile at the
//       reference commit, UD-733; the test is UNSTABLE, never built by
//       default) (MeshAccessor hexahedron, default
//       initFull = explicit lumped mass, Δt = 0.5·getStableTimeStep(),
//       4666 steps of the piecewise displacement program); per step σ_xx of
//       quadrature point 0 and getEnergy("potential" / "kinetic" /
//       "dissipated" / "external work") in upstream's call order; the
//       upstream criteria's inputs (σ at step 2·nb/8, σ_max, σ_min, the
//       softening check), and the final nodal and quadrature-point state.
//       No linear solve (explicit lumped): unaffected by Eigen/OpenBLAS.
//   material_gtest_random_rotation
//       FriendMaterial<T>::getRandomVector / getRandomRotation of
//       test_material_fixtures.hh transcribed (std::mt19937 seeded with the
//       gtest default random_seed 0, std::uniform_real_distribution<Real>),
//       first rotation of a fresh generator for dimensions 2 and 3 (each
//       typed test builds its own FriendMaterial), plus the first 2000 raw
//       std::mt19937(0) outputs and 1000 uniform draws (black-box data for
//       the clean-room engine of the port, D-030).
//   elasto_plastic_linear_isotropic_hardening
//       test_material_elasto_plastic_linear_isotropic_hardening.cc with the
//       Eigen sparse solver pinned (upstream: the default solver): per step
//       the printed reaction (value), the Newton iteration count and error.
//
// Usage: test2b_probe <fixtures_root> <upstream_src_root> <frozen_meshes>
//                     <work_dir>
// Environment: T2B_PROBE_CASES=<id>[,<id>...] restricts the run.

#include "akantu_probe/fixture_writer.hh"

// Eigen comes with the Akantu headers (its plugins must be defined before
// Eigen is included: no direct <Eigen/Core> include here).
#include <aka_common.hh>
#include <aka_iterators.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <non_linear_solver.hh>
#include <solid_mechanics_model.hh>

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>
#include <random>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "test2b_probe";
constexpr const char * kProbeVersion = "2"; // 2: valid kind of rotation/*

using I64 = std::int64_t;

fs::path g_root, g_up, g_frozen, g_work;

bool wanted(const std::string & id) {
  const char * env = std::getenv("T2B_PROBE_CASES");
  if (env == nullptr || *env == '\0') {
    return true;
  }
  std::stringstream list(env);
  std::string item;
  while (std::getline(list, item, ',')) {
    if (item == id) {
      return true;
    }
  }
  return false;
}

std::vector<double> vec_of(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a))
      .description(desc);
}

void add_vec(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
             const std::string & desc) {
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
}

void add_ivec(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
              const std::string & desc) {
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
}

/// Record the x86_64 pins of D-050 in the case parameters.
void pin_params(ap::Json & p) {
  p["eigen_cpu_cache_sizes"] = "16384,524288,524288 (Eigen::setCpuCacheSizes in main)";
  const char * core = std::getenv("OPENBLAS_CORETYPE");
  const char * threads = std::getenv("OPENBLAS_NUM_THREADS");
  p["openblas_coretype"] = core != nullptr ? core : "";
  p["openblas_num_threads"] = threads != nullptr ? threads : "";
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

/* -------------------------------------------------------------------------- */
/* test_material_mazars.cc                                                    */
/* -------------------------------------------------------------------------- */
void mazars_case() {
  const std::string id = "material_mazars_hexahedron_8";
  if (not wanted(id)) {
    return;
  }
  auto dat = fs::path(g_up) / "test/test_model/test_solid_mechanics_model/test_materials/"
                              "material_mazars.dat";
  getStaticParser().parse(dat.string());
  const Int spatial_dimension = 3;
  ElementType type = _hexahedron_8;
  Mesh mesh(spatial_dimension);
  MeshAccessor mesh_accessor(mesh);
  Array<Real> & nodes = mesh_accessor.getNodes();
  Array<Idx> & connectivity = mesh_accessor.getConnectivity(type);
  const Real width = 1;
  Int nb_dof = 8;
  connectivity.resize(1);
  nodes.resize(nb_dof);
  const Real xyz[8][3] = {{0., 0., 0.},       {width, 0., 0.},   {width, width, 0},
                          {0, width, 0},      {0., 0., width},   {width, 0., width},
                          {width, width, width}, {0, width, width}};
  for (Int n = 0; n < 8; ++n) {
    for (Int d = 0; d < 3; ++d) {
      nodes(n, d) = xyz[n][d];
    }
    connectivity(0, n) = n;
  }
  mesh_accessor.makeReady();

  SolidMechanicsModel model(mesh);
  model.initFull();

  Array<Real> & disp = model.getDisplacement();
  Array<Real> & velo = model.getVelocity();
  Array<bool> & boun = model.getBlockedDOFs();
  for (Int i = 0; i < nb_dof; ++i) {
    boun(i, 0) = true;
  }
  Real time_step = model.getStableTimeStep() * .5;
  model.setTimeStep(time_step);

  Material & mat = model.getMaterial(0);
  const Array<Real> & stress = mat.getStress(type);
  const Array<Real> & damage = mat.getArray<Real>("damage", type);

  Real wext = 0.;
  Real sigma_max = 0, sigma_min = 0;
  Real max_disp = 0;
  Real stress_xx_compression_1 = 0;
  UInt nb_steps = 7e5 / 150;
  Real adisp = 0;
  bool softening_failed = false;
  std::vector<double> sxx, epot_v, ekin_v, edis_v, wext_v;
  for (Int s = 0; s < nb_steps; ++s) {
    if (s == 0) {
      max_disp = 0.003;
      adisp = -(max_disp * 8. / nb_steps) / 2.;
    }
    if (s == (2 * nb_steps / 8)) {
      stress_xx_compression_1 = stress(0, 0);
      max_disp = 0 + max_disp;
      adisp = max_disp * 8. / nb_steps;
    }
    if (s == (3 * nb_steps / 8)) {
      max_disp = 0.004;
      adisp = -max_disp * 8. / nb_steps;
    }
    if (s == (4 * nb_steps / 8)) {
      if (stress(0, 0) < stress_xx_compression_1) {
        softening_failed = true;
        break; // upstream: return EXIT_FAILURE
      }
      max_disp = 0.0015 + max_disp;
      adisp = max_disp * 8. / nb_steps;
    }
    if (s == (5 * nb_steps / 8)) {
      max_disp = 0. + 0.0005;
      adisp = -max_disp * 8. / nb_steps;
    }
    if (s == (6 * nb_steps / 8)) {
      max_disp = 0.0035 - 0.001;
      adisp = max_disp * 8. / nb_steps;
    }
    if (s == (7 * nb_steps / 8)) {
      adisp = -max_disp * 8. / nb_steps;
    }
    for (Int i = 0; i < nb_dof; ++i) {
      if (std::abs(nodes(i, 0) - width) < std::numeric_limits<Real>::epsilon()) {
        disp(i, 0) += adisp;
        velo(i, 0) = adisp / time_step;
      }
    }
    model.solveStep();
    Real epot = model.getEnergy("potential");
    Real ekin = model.getEnergy("kinetic");
    Real edis = model.getEnergy("dissipated");
    wext += model.getEnergy("external work");
    sigma_max = std::max(sigma_max, stress(0, 0));
    sigma_min = std::min(sigma_min, stress(0, 0));
    sxx.push_back(stress(0, 0));
    epot_v.push_back(epot);
    ekin_v.push_back(ekin);
    edis_v.push_back(edis);
    wext_v.push_back(wext);
  }
  const bool upstream_pass = (not softening_failed) and
                             not((std::abs(sigma_max) > std::abs(sigma_min)) or
                                 (std::abs(sigma_max - 6.24e6) > 1e5) or
                                 (std::abs(sigma_min + 2.943e7) > 1e6));

  ap::FixtureWriter w(g_root, "model/upstream_tests_2b/" + id,
                      "test_material_mazars.cc replayed: per-step stress_xx(q0) and energies, "
                      "upstream criteria inputs, final state",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("upstream_test");
  w.add_input_file("material", dat, "test/test_model/test_solid_mechanics_model/test_materials/"
                                    "material_mazars.dat");
  auto & p = w.parameters();
  pin_params(p);
  p["upstream_test"] = "test_material_mazars";
  p["nb_steps"] = static_cast<long long>(nb_steps);
  p["time_step"] = time_step;
  p["recorded_steps"] = static_cast<long long>(sxx.size());
  p["softening_failed"] = softening_failed;
  p["stress_xx_compression_1"] = stress_xx_compression_1;
  p["sigma_max"] = sigma_max;
  p["sigma_min"] = sigma_min;
  p["upstream_criteria_pass"] = upstream_pass;
  add_vec(w, "step/stress_xx", sxx, "stress(0, 0) after solveStep (quadrature point 0)");
  add_vec(w, "step/energy/potential", epot_v, "getEnergy(\"potential\") after solveStep");
  add_vec(w, "step/energy/kinetic", ekin_v, "getEnergy(\"kinetic\") after solveStep");
  add_vec(w, "step/energy/dissipated", edis_v, "getEnergy(\"dissipated\") after solveStep");
  add_vec(w, "step/energy/external_work_sum", wext_v,
          "running sum of getEnergy(\"external work\") (upstream wext)");
  add_array(w, "final/displacement", disp, "displacement after the last step");
  add_array(w, "final/velocity", velo, "velocity after the last step");
  add_array(w, "final/stress", stress, "stress of every quadrature point after the last step");
  add_array(w, "final/damage", damage, "damage of every quadrature point after the last step");
  add_array(w, "final/mass", model.getMass(), "lumped mass");
  w.finish();
  std::cout << "[" << kProbe << "] " << id << ": sigma_max = " << sigma_max
            << ", sigma_min = " << sigma_min << ", softening_failed = " << softening_failed
            << ", upstream criteria " << (upstream_pass ? "pass" : "FAIL") << std::endl;
}

/* -------------------------------------------------------------------------- */
/* test_material_fixtures.hh: getRandomVector / getRandomRotation             */
/* -------------------------------------------------------------------------- */
/// Transcription of FriendMaterial<T>::getRandomVector.
Vector<Real> random_vector(std::mt19937 & gen, Int dim) {
  std::uniform_real_distribution<Real> dis;
  Vector<Real> vector(dim);
  vector.zero();
  while (vector.norm() < 1e-9) {
    for (auto s : arange(dim)) {
      vector(s) = dis(gen);
    }
  }
  return vector;
}

/// Transcription of FriendMaterial<T>::getRandomRotation.
Matrix<Real> random_rotation(std::mt19937 & gen, Int dim) {
  Matrix<Real> rotation(dim, dim);
  auto && v1_ = rotation(0);
  v1_ = random_vector(gen, dim).normalized();
  if (dim == 2) {
    Vector<Real, 3> v1{v1_(0), v1_(1), 0};
    Vector<Real, 3> v3{0, 0, 1};
    Vector<Real, 3> v2 = v3.cross(v1);
    rotation(1) = v2.block<2, 1>(0, 0);
  }
  if (dim == 3) {
    Vector<Real, 3> v2 = random_vector(gen, dim);
    Vector<Real, 3> v1 = v1_;
    v2 = (v2 - v2.dot(v1) * v1).normalized();
    Vector<Real, 3> v3 = v1.cross(v2);
    rotation(1) = v2;
    rotation(2) = v3;
  }
  rotation.transposeInPlace();
  return rotation;
}

void rotation_case() {
  const std::string id = "material_gtest_random_rotation";
  if (not wanted(id)) {
    return;
  }
  ap::FixtureWriter w(g_root, "model/upstream_tests_2b/" + id,
                      "test_material_fixtures.hh random rotations (std::mt19937 seed 0) and raw "
                      "engine / uniform_real_distribution draws",
                      kProbe, kProbeVersion);
  w.add_tag("random");
  w.add_tag("upstream_test");
  auto & p = w.parameters();
  pin_params(p);
  p["seed"] = 0LL;
  p["engine"] = "std::mt19937";
  p["distribution"] = "std::uniform_real_distribution<Real>() (a = 0, b = 1)";
  for (Int dim : {2, 3}) {
    std::mt19937 gen;
    gen.seed(0);
    auto r = random_rotation(gen, dim);
    std::vector<double> v(r.data(), r.data() + dim * dim);
    w.add("rotation/" + std::to_string(dim), {static_cast<std::size_t>(dim * dim)}, {"entry"},
          "table", v)
        .description("first getRandomRotation() of a fresh FriendMaterial (column-major "
                     "storage order)");
  }
  {
    std::mt19937 gen;
    gen.seed(0);
    std::vector<I64> raw;
    for (int i = 0; i < 2000; ++i) {
      raw.push_back(static_cast<I64>(gen()));
    }
    add_ivec(w, "engine/raw", raw, "first 2000 outputs of std::mt19937 after seed(0)");
  }
  {
    std::mt19937 gen;
    gen.seed(0);
    std::uniform_real_distribution<Real> dis;
    std::vector<double> u;
    for (int i = 0; i < 1000; ++i) {
      u.push_back(dis(gen));
    }
    add_vec(w, "engine/uniform", u, "first 1000 uniform_real_distribution<Real>() draws");
  }
  w.finish();
  std::cout << "[" << kProbe << "] " << id << std::endl;
}

/* -------------------------------------------------------------------------- */
/* test_material_elasto_plastic_linear_isotropic_hardening.cc                 */
/* -------------------------------------------------------------------------- */
void elasto_plastic_case() {
  const std::string id = "elasto_plastic_linear_isotropic_hardening";
  if (not wanted(id)) {
    return;
  }
  const std::string rel = "test/test_model/test_solid_mechanics_model/test_materials/"
                          "test_material_elasto_plastic_linear_isotropic_hardening/";
  auto dat = fs::path(g_up) / (rel + "test_material_elasto_plastic_linear_isotropic_hardening.dat");
  auto msh = fs::path(g_frozen) / (rel + "test_material_elasto_plastic_linear_isotropic_hardening.msh");
  getStaticParser().parse(dat.string());
  const Int spatial_dimension = 2;
  const Real u_increment = 0.1;
  const Int steps = 20;
  Mesh mesh(spatial_dimension);
  mesh.read(msh.string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver("static");
  solver.set("max_iterations", 300);
  solver.set("threshold", 1e-5);
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "bottom");
  std::vector<double> reactions, errors, strains;
  std::vector<I64> iterations;
  for (Int i = 0; i < steps; ++i) {
    model.applyBC(BC::Dirichlet::FixedValue(i * u_increment, _x), "right");
    model.solveStep(); // a non-converged step throws, as upstream (rethrow)
    Real strainxx = i * u_increment / 10.;
    const auto & edge_nodes = mesh.getElementGroup("right").getNodeGroup().getNodes();
    auto & residual = model.getInternalForce();
    Real reaction = 0;
    for (Int n = 0; n < edge_nodes.size(); n++) {
      reaction -= residual(edge_nodes(n), 0);
    }
    strains.push_back(strainxx);
    reactions.push_back(reaction);
    iterations.push_back(static_cast<I64>(Int(solver.get("nb_iterations"))));
    errors.push_back(Real(solver.get("error")));
  }
  ap::FixtureWriter w(g_root, "model/upstream_tests_2b/" + id,
                      "test_material_standard_linear_isotropic_hardening replayed with the Eigen "
                      "sparse solver: reactions, Newton iterations and errors",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("upstream_test");
  w.add_input_file("mesh", msh, "frozen Gmsh 4.13.1 mesh of the upstream add_mesh target (D-009)");
  w.add_input_file("material", dat, rel + "test_material_elasto_plastic_linear_isotropic_hardening.dat");
  auto & p = w.parameters();
  pin_params(p);
  p["upstream_test"] = "test_material_standard_linear_isotropic_hardening";
  p["sparse_solver"] = "eigen (pinned; upstream uses the default)";
  p["steps"] = static_cast<long long>(steps);
  add_vec(w, "step/strain_xx", strains, "printed strain i * u_increment / 10");
  add_vec(w, "step/reaction", reactions, "printed reaction -sum f_int,x over the right node group");
  add_ivec(w, "step/nb_iterations", iterations, "Newton iterations per solveStep");
  add_vec(w, "step/error", errors, "Newton error per solveStep");
  add_array(w, "final/displacement", model.getDisplacement(), "displacement after the last step");
  add_array(w, "final/internal_force", model.getInternalForce(),
            "internal force after the last step");
  w.finish();
  std::cout << "[" << kProbe << "] " << id << ": final reaction " << reactions.back()
            << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  // x86_64 oracle pinned like the aarch64 reference (D-050)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (argc < 5) {
    std::cerr << "usage: " << argv[0]
              << " <fixtures_root> <upstream_src_root> <frozen_meshes> <work_dir>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  g_work = argv[4];
  fs::create_directories(g_work);
  int fake_argc = 1;
  char * fake_argv[] = {argv[0], nullptr};
  char ** fake = fake_argv;
  initialize(fake_argc, fake);
  try {
    rotation_case();
    mazars_case();
    elasto_plastic_case();
  } catch (std::exception & e) {
    std::cerr << "test2b_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
