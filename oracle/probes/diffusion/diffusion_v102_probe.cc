// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) written by the independent reviewer,
// adopted for the committed fixtures
// fixtures/model/diffusion/v102_* (oracle/probes/diffusion/generate.sh with
// DIFF_PROBE_SET=v102). Changes against the reviewer's file: case ids
// model/diffusion/v102_<case> (were v102/<case>), the D-046 NaN inventory
// written into the notes of a case whose recorded values hold NaN, and the
// inputs of ud283_cohesive_init recorded. Runs the unmodified
// upstream HeatTransferModel (src/model/diffusion_model/*) on adversarial
// cases the author's probe (oracle/probes/diffusion/diffusion_probe.cc) does
// not cover, and on the UD-287 / UD-283 questions of the review brief.
//
// Cases (case ids model/diffusion/v102_<case>, fixture format v1 of akantu_probe):
//   ud287_bar           1-D bar u_t = u_xx on [0,1], u(0)=u(1)=0, rho c = 1,
//                       k = 1, t_end = 0.1; backward Euler (alpha = 1) and
//                       trapezoidal rule 1 (alpha = 1/2) in a solver of their
//                       own; drivers "plain" (solveStep only, as every
//                       upstream example), "shift" (before each solveStep
//                       the free temperatures get + alpha dt T_dot: a
//                       driver-level emulation of the textbook predictor on
//                       the unmodified oracle) and, for backward Euler,
//                       "rate" (solution type _temperature_rate); initial
//                       data "sine" (sin(pi x), a discrete eigenvector) and
//                       "poly" (4 x (1 - x)); time refinement S = 10..160 at
//                       N = 16 and space-time refinement N = 8..128.
//   aniso2d_<analysis>  mixed _quadrangle_4 / _triangle_3 mesh (perturbed),
//                       one law with a full anisotropic conductivity.
//   aniso3d_<analysis>  mixed _hexahedron_8 / _pentahedron_6 mesh, full 3x3
//                       anisotropic conductivity.
//   rate0_<analysis>    non-zero, non-consistent initial temperature rate.
//   kink_<analysis>     temperature-dependent conductivity whose diagonal
//                       crosses zero inside the domain (T crosses T_ref).
//   zerorho_<analysis>  density = 0 (accepted upstream).
//   ud281_lumped        explicit solve after assembleCapacityMatrix().
//   ud283_cohesive_dt   stable time step before / after a _cohesive_2d_4
//                       element is appended to the mesh (MeshAccessor).
//   ud283_cohesive_init (separate run) model built on a mesh that already
//                       holds a cohesive element.
//
// Usage: v102_probe <fixtures_root> <upstream_src_root>
// Environment: V102_CASES=<id>[,<id>...] restricts the run.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <heat_transfer_model.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <non_linear_solver.hh>
#include <non_linear_solver_newton_raphson.hh>
#include <sparse_matrix_aij.hh>

#include <cmath>
#include <utility>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <set>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "v102_probe";
constexpr const char * kProbeVersion = "1";
using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up;

bool wanted(const std::string & id) {
  const char * env = std::getenv("V102_CASES");
  if (env == nullptr || *env == '\0') {
    return id != "ud283_cohesive_init"; // separate run (may abort)
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
std::vector<std::uint8_t> vec_of(const Array<bool> & a) {
  std::vector<std::uint8_t> v(a.size() * a.getNbComponent());
  for (std::size_t i = 0; i < v.size(); ++i) {
    v[i] = a.data()[i] ? 1 : 0;
  }
  return v;
}
// D-046 (4): NaN count per recorded quantity of the current case
std::vector<std::pair<std::string, std::size_t>> g_nans;

void add_vec(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
             const std::string & desc) {
  std::size_t n = 0;
  for (double x : v) {
    n += std::isnan(x) ? 1 : 0;
  }
  if (n > 0) {
    g_nans.emplace_back(name, n);
  }
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
}

// The structured NaN inventory of the case's notes (D-046 (4), parsed by
// crates/akantu-validation/tests/oracle_fixtures.rs): "NaN quantities
// (count of N values): q1 (n1), q2 (n2); <why>".
void set_nan_notes(ap::FixtureWriter & w, const std::string & why) {
  if (g_nans.empty()) {
    return;
  }
  std::size_t total = 0;
  std::string list;
  for (auto & [q, n] : g_nans) {
    total += n;
    list += (list.empty() ? "" : ", ") + q + " (" + std::to_string(n) + ")";
  }
  w.set_notes("D-046: the oracle run produces NaN by design of the case (" + why +
              "); every NaN is the platform's default quiet NaN. NaN quantities (count of " +
              std::to_string(total) + " values): " + list + ";");
  g_nans.clear();
}
void add_ivec(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
              const std::string & desc) {
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
}
void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  add_vec(w, name, vec_of(a), desc);
}
void add_bools(ap::FixtureWriter & w, const std::string & name, const Array<bool> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size())}, {"entry"}, "table", vec_of(a))
      .description(desc);
}

std::string num(double x) {
  char b[64];
  std::snprintf(b, sizeof b, "%.17g", x);
  return b;
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

void set_newton(HeatTransferModel & model, const ID & id, Int max_it, Real threshold,
                SolveConvergenceCriteria crit) {
  auto & solver = model.getNonLinearSolver(id);
  if (aka::is_of_type<NonLinearSolverNewtonRaphson>(solver)) {
    solver.set("max_iterations", max_it);
    solver.set("threshold", threshold);
    solver.set("convergence_type", crit);
  }
}

bool is_newton(HeatTransferModel & model, const ID & id = "") {
  return aka::is_of_type<NonLinearSolverNewtonRaphson>(model.getNonLinearSolver(id));
}

void write_file(const fs::path & p, const std::string & s) {
  std::ofstream f(p, std::ios::binary | std::ios::trunc);
  f << s;
}

/// COO of an AIJ matrix (storage order).
void record_aij(ap::FixtureWriter & w, const std::string & prefix, HeatTransferModel & model,
                const ID & id) {
  auto & p = w.parameters();
  if (not model.getDOFManager().hasMatrix(id)) {
    p[prefix + "/exists"] = false;
    return;
  }
  auto & A = aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix(id));
  const auto nnz = A.getNbNonZero();
  std::vector<I64> rows(nnz), cols(nnz);
  std::vector<double> vals(nnz);
  for (Int k = 0; k < nnz; ++k) {
    rows[k] = A.getIRN()(k) - 1;
    cols[k] = A.getJCN()(k) - 1;
    vals[k] = A.getA()(k);
  }
  p[prefix + "/exists"] = true;
  add_ivec(w, prefix + "/rows", rows, "getIRN - 1");
  add_ivec(w, prefix + "/cols", cols, "getJCN - 1");
  add_vec(w, prefix + "/values", vals, "getA");
}

void record_lumped(ap::FixtureWriter & w, HeatTransferModel & model) {
  auto & dm = model.getDOFManager();
  if (not dm.hasLumpedMatrix("M")) {
    w.parameters()["lumped_M/exists"] = false;
    return;
  }
  Array<Real> m(model.getTemperature().size(), 1);
  m.zero();
  dm.getLumpedMatrixPerDOFs("temperature", "M", m);
  w.parameters()["lumped_M/exists"] = true;
  add_array(w, "lumped_M", m, "getLumpedMatrixPerDOFs(temperature, M)");
}

/* -------------------------------------------------------------------------- */
/* meshes                                                                     */
/* -------------------------------------------------------------------------- */
std::string bar_mesh(Int n) {
  std::ostringstream s;
  s << "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n" << n + 1 << "\n";
  for (Int i = 0; i <= n; ++i) {
    s << i + 1 << " " << num(Real(i) / Real(n)) << " 0 0\n";
  }
  s << "$EndNodes\n$Elements\n" << n << "\n";
  for (Int e = 0; e < n; ++e) {
    s << e + 1 << " 1 2 1 1 " << e + 1 << " " << e + 2 << "\n";
  }
  s << "$EndElements\n";
  return s.str();
}

/// 4 x 3 cells on [0, 2] x [0, 1.5]: columns 0-1 quadrangles, columns 2-3
/// triangles (two per cell, alternating diagonals); interior nodes
/// perturbed.
std::string mixed2d_mesh() {
  const Int nx = 4, ny = 3;
  std::ostringstream s;
  s << "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n" << (nx + 1) * (ny + 1) << "\n";
  auto id = [&](Int i, Int j) { return j * (nx + 1) + i + 1; };
  for (Int j = 0; j <= ny; ++j) {
    for (Int i = 0; i <= nx; ++i) {
      Real x = 0.5 * i, y = 0.5 * j;
      if (i > 0 && i < nx && j > 0 && j < ny) {
        x += 0.07 * ((i + 2 * j) % 3 - 1);
        y += 0.05 * ((2 * i + j) % 3 - 1);
      }
      s << id(i, j) << " " << num(x) << " " << num(y) << " 0\n";
    }
  }
  s << "$EndNodes\n";
  std::vector<std::string> els;
  for (Int j = 0; j < ny; ++j) {
    for (Int i = 0; i < nx; ++i) {
      Int a = id(i, j), b = id(i + 1, j), c = id(i + 1, j + 1), d = id(i, j + 1);
      if (i < 2) {
        els.push_back("3 2 1 1 " + std::to_string(a) + " " + std::to_string(b) + " " +
                      std::to_string(c) + " " + std::to_string(d));
      } else if ((i + j) % 2 == 0) {
        els.push_back("2 2 2 2 " + std::to_string(a) + " " + std::to_string(b) + " " +
                      std::to_string(c));
        els.push_back("2 2 2 2 " + std::to_string(a) + " " + std::to_string(c) + " " +
                      std::to_string(d));
      } else {
        els.push_back("2 2 2 2 " + std::to_string(a) + " " + std::to_string(b) + " " +
                      std::to_string(d));
        els.push_back("2 2 2 2 " + std::to_string(b) + " " + std::to_string(c) + " " +
                      std::to_string(d));
      }
    }
  }
  s << "$Elements\n" << els.size() << "\n";
  for (std::size_t e = 0; e < els.size(); ++e) {
    s << e + 1 << " " << els[e] << "\n";
  }
  s << "$EndElements\n";
  return s.str();
}

/// x in {0, 1, 2}, y in {0, 1}, z in {0, 0.5, 1}: per z layer one
/// hexahedron on x in [0, 1] and two prisms on x in [1, 2].
std::string mixed3d_mesh() {
  const Int nx = 2, ny = 1, nz = 2;
  auto id = [&](Int i, Int j, Int k) { return (k * (ny + 1) + j) * (nx + 1) + i + 1; };
  std::ostringstream s;
  s << "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n" << (nx + 1) * (ny + 1) * (nz + 1)
    << "\n";
  for (Int k = 0; k <= nz; ++k) {
    for (Int j = 0; j <= ny; ++j) {
      for (Int i = 0; i <= nx; ++i) {
        s << id(i, j, k) << " " << num(Real(i)) << " " << num(Real(j)) << " "
          << num(0.5 * k) << "\n";
      }
    }
  }
  s << "$EndNodes\n";
  std::vector<std::string> els;
  auto str = [](std::initializer_list<Int> l) {
    std::string r;
    for (auto v : l) {
      r += " " + std::to_string(v);
    }
    return r;
  };
  for (Int k = 0; k < nz; ++k) {
    els.push_back("5 2 1 1" + str({id(0, 0, k), id(1, 0, k), id(1, 1, k), id(0, 1, k),
                                   id(0, 0, k + 1), id(1, 0, k + 1), id(1, 1, k + 1),
                                   id(0, 1, k + 1)}));
    els.push_back("6 2 2 2" + str({id(1, 0, k), id(2, 0, k), id(2, 1, k), id(1, 0, k + 1),
                                   id(2, 0, k + 1), id(2, 1, k + 1)}));
    els.push_back("6 2 2 2" + str({id(1, 0, k), id(2, 1, k), id(1, 1, k), id(1, 0, k + 1),
                                   id(2, 1, k + 1), id(1, 1, k + 1)}));
  }
  s << "$Elements\n" << els.size() << "\n";
  for (std::size_t e = 0; e < els.size(); ++e) {
    s << e + 1 << " " << els[e] << "\n";
  }
  s << "$EndElements\n";
  return s.str();
}

std::string heat_dat(const std::string & conductivity, Real density, Real capacity,
                     Real variation, Real t_ref) {
  return "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = v102\n"
         "    density = " +
         num(density) + "\n    capacity = " + num(capacity) +
         "\n    conductivity = " + conductivity + "\n    conductivity_variation = " +
         num(variation) + "\n    temperature_reference = " + num(t_ref) + "\n  ]\n]\n";
}

/* -------------------------------------------------------------------------- */
/* UD-287: 1-D bar                                                            */
/* -------------------------------------------------------------------------- */
struct BarRun {
  std::vector<double> T;
  I64 iterations{0};
  double max_error{0.};
  std::string exception;
};

/// One run: `scheme` = "be" | "cn", `driver` = "plain" | "shift" | "rate",
/// `ic` = "sine" | "poly".
BarRun bar_run(const fs::path & mesh_file, const fs::path & dat, Int n, Int steps,
               const std::string & scheme, const std::string & driver, const std::string & ic,
               std::vector<double> * T0_out) {
  BarRun out;
  Mesh mesh(1);
  mesh.read(mesh_file.string());
  getStaticParser().parse(dat.string());
  HeatTransferModel model(mesh);
  // static default solver: a second dynamic solver would register the rate
  // twice and throw (UD-012, hasDOFsDerivatives off by one)
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  ModelSolverOptions o = eigen_options();
  o.timestep_solver_type = TimeStepSolverType::_dynamic;
  o.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  const ID sid = "under_test";
  model.getNewSolver(sid, o);
  const Real alpha = scheme == "be" ? 1. : 0.5;
  model.setIntegrationScheme(sid, "temperature",
                             scheme == "be" ? IntegrationSchemeType::_backward_euler
                                            : IntegrationSchemeType::_trapezoidal_rule_1,
                             driver == "rate" ? IntegrationScheme::_temperature_rate
                                              : IntegrationScheme::_temperature);
  set_newton(model, sid, 10, 1e-10, SolveConvergenceCriteria::_solution);
  const Real dt = 0.1 / Real(steps);
  model.setTimeStep(dt, sid);
  auto & T = model.getTemperature();
  auto & blocked = model.getBlockedDOFs();
  const auto & X = mesh.getNodes();
  for (Int i = 0; i <= n; ++i) {
    const Real x = X(i, 0);
    T(i) = ic == "sine" ? std::sin(M_PI * x) : 4. * x * (1. - x);
  }
  blocked(0) = true;
  blocked(n) = true;
  T(0) = 0.;
  T(n) = 0.;
  if (T0_out != nullptr) {
    *T0_out = vec_of(T);
  }
  auto & rate = model.getDOFManager().getDOFsDerivatives("temperature", 1);
  auto & nls = model.getNonLinearSolver(sid);
  try {
    for (Int s = 0; s < steps; ++s) {
      if (driver == "shift") {
        for (Int i = 0; i <= n; ++i) {
          if (not blocked(i)) {
            T(i) += alpha * dt * rate(i);
          }
        }
      }
      model.solveStep(sid);
      out.iterations += Int(nls.get("nb_iterations"));
      out.max_error = std::max(out.max_error, Real(nls.get("error")));
    }
  } catch (std::exception & e) {
    out.exception = e.what();
  }
  out.T = vec_of(T);
  return out;
}

void ud287_bar() {
  const std::string cid = "model/diffusion/v102_ud287_bar";
  if (not wanted("ud287_bar")) {
    return;
  }
  g_nans.clear();
  ap::FixtureWriter w(g_root, cid,
                      "UD-287: 1-D bar u_t = u_xx, generalized trapezoidal alpha = 1 / 0.5, "
                      "plain / shift / rate drivers (review V102)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("diffusion");
  auto dat = fs::temp_directory_path() / "v102_bar.dat";
  write_file(dat, heat_dat("[[1.0]]", 1., 1., 0., 0.));
  w.add_input_file("material", dat, "generated:v102_probe.cc");
  auto & p = w.parameters();
  Json runs = Json::array();
  std::vector<Int> meshes_written;
  std::set<std::string> t0_written;
  long nb_runs = 0;
  auto mesh_of = [&](Int n) {
    auto f = fs::temp_directory_path() / ("v102_bar_" + std::to_string(n) + ".msh");
    write_file(f, bar_mesh(n));
    bool seen = false;
    for (auto m : meshes_written) {
      seen |= m == n;
    }
    if (not seen) {
      w.add_input_file("mesh_N" + std::to_string(n), f, "generated:v102_probe.cc");
      meshes_written.push_back(n);
    }
    return f;
  };
  auto one = [&](Int n, Int steps, const std::string & scheme, const std::string & driver,
                 const std::string & ic) {
    const std::string key = scheme + "/" + driver + "/" + ic + "/N" + std::to_string(n) + "/S" +
                            std::to_string(steps);
    std::vector<double> T0;
    auto r = bar_run(mesh_of(n), dat, n, steps, scheme, driver, ic, &T0);
    add_vec(w, key + "/T", r.T, "final temperature");
    const std::string k0 = "T0/" + ic + "/N" + std::to_string(n);
    if (t0_written.insert(k0).second) {
      add_vec(w, k0, T0, "initial temperature (blocked ends 0)");
    }
    Json rj = Json::object();
    rj["key"] = key;
    rj["T0"] = k0;
    rj["N"] = static_cast<long long>(n);
    rj["S"] = static_cast<long long>(steps);
    rj["scheme"] = scheme;
    rj["driver"] = driver;
    rj["ic"] = ic;
    rj["iterations"] = static_cast<long long>(r.iterations);
    rj["max_newton_error"] = r.max_error;
    if (not r.exception.empty()) {
      rj["exception"] = r.exception;
    }
    runs.push(rj);
    ++nb_runs;
  };
  for (const char * ic : {"sine", "poly"}) {
    for (Int steps : {10, 20, 40, 80, 160}) {
      for (const char * scheme : {"be", "cn"}) {
        for (const char * driver : {"plain", "shift"}) {
          one(16, steps, scheme, driver, ic);
        }
      }
      one(16, steps, "be", "rate", ic);
    }
    for (Int n : {8, 16, 32, 64, 128}) {
      // backward Euler: dt ~ h^2 (S = N^2 / 4); trapezoidal: dt ~ h (S = N)
      for (const char * driver : {"plain", "shift"}) {
        one(n, n * n / 4, "be", driver, ic);
        one(n, n, "cn", driver, ic);
      }
    }
  }
  p["runs"] = runs;
  p["t_end"] = 0.1;
  p["newton"] = "max_iterations 10, threshold 1e-10, solution criterion";
  set_nan_notes(w, "unexpected: no reason recorded");
  w.finish();
  std::cout << "  " << cid << " (" << nb_runs << " runs)" << std::endl;
}

/* -------------------------------------------------------------------------- */
/* generic transient / static case on a written mesh                          */
/* -------------------------------------------------------------------------- */
struct CaseSetup {
  std::string id, description;
  Int dim;
  std::string mesh_text;          // written mesh, or empty for upstream_mesh
  std::string upstream_mesh;      // relative to test/test_model/patch_tests/data
  std::string dat;
  AnalysisMethod method;
  Real dt_factor;                 // dt = dt_factor * stable (if dt_fixed == 0)
  Real dt_fixed{0.};
  Int steps;
  // nodal initialisation
  std::function<void(const Vector<Real> &, Real &, bool &, Real &)> init;
  bool set_rate{false};
  bool consistent_first{false};   // ud281: create M + assembleCapacityMatrix()
  std::string nan_why{};          // why the case may record NaN (D-046)
};

void run_case(const CaseSetup & c) {
  const std::string cid = "model/diffusion/v102_" + c.id;
  if (not wanted(c.id)) {
    return;
  }
  fs::path mesh_file;
  if (c.mesh_text.empty()) {
    mesh_file = g_up / "test/test_model/patch_tests/data" / c.upstream_mesh;
  } else {
    mesh_file = fs::temp_directory_path() / ("v102_" + c.id + ".msh");
    write_file(mesh_file, c.mesh_text);
  }
  auto dat = fs::temp_directory_path() / ("v102_" + c.id + ".dat");
  write_file(dat, c.dat);
  ap::FixtureWriter w(g_root, cid, c.description, kProbe, kProbeVersion);
  g_nans.clear();
  w.add_tag("model");
  w.add_tag("diffusion");
  w.add_input_file("mesh", mesh_file,
                   c.mesh_text.empty()
                       ? "upstream:test/test_model/patch_tests/data/" + c.upstream_mesh
                       : "generated:v102_probe.cc");
  w.add_input_file("material", dat, "generated:v102_probe.cc");
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(c.dim);
  std::string analysis = c.method == _static              ? "static"
                         : c.method == _implicit_dynamic ? "implicit"
                                                          : "explicit";
  p["analysis"] = analysis;
  p["steps"] = static_cast<long long>(c.steps);
  Mesh mesh(c.dim);
  mesh.read(mesh_file.string());
  getStaticParser().parse(dat.string());
  HeatTransferModel model(mesh);
  std::vector<std::string> stage;
  bool failed = false;
  try {
    stage.push_back("initFull");
    model.initFull(_analysis_method = c.method, _solver_options = eigen_options());
    stage.push_back("getStableTimeStep");
    const Real stable = model.getStableTimeStep();
    Real dt = c.dt_fixed != 0. ? c.dt_fixed : c.dt_factor * stable;
    add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
    if (c.method != _static) {
      model.setTimeStep(dt);
    }
    if (is_newton(model)) {
      set_newton(model, "", 30, 1e-10, SolveConvergenceCriteria::_solution);
    }
    auto & T = model.getTemperature();
    auto & blocked = model.getBlockedDOFs();
    const auto & X = mesh.getNodes();
    Array<Real> * rate = nullptr;
    if (c.method != _static) {
      rate = &model.getDOFManager().getDOFsDerivatives("temperature", 1);
    }
    for (Int i = 0; i < mesh.getNbNodes(); ++i) {
      Vector<Real> x(c.dim);
      for (Int d = 0; d < c.dim; ++d) {
        x(d) = X(i, d);
      }
      Real r = 0.;
      bool b = false;
      c.init(x, T(i), b, r);
      blocked(i) = b;
      if (c.set_rate && rate != nullptr) {
        (*rate)(i) = r;
      }
    }
    add_array(w, "initial_temperature", T, "initial temperature");
    add_bools(w, "blocked_dofs", blocked, "blocked flags");
    if (rate != nullptr) {
      add_array(w, "initial_rate", *rate, "initial temperature rate");
    }
    if (c.consistent_first) {
      stage.push_back("assembleCapacityMatrix");
      if (not model.getDOFManager().hasMatrix("M")) {
        model.getDOFManager().getNewMatrix("M", _symmetric);
      }
      model.assembleCapacityMatrix();
    }
    std::vector<I64> its;
    std::vector<double> errs, energies;
    for (Int s = 0; s < c.steps; ++s) {
      stage.push_back("solveStep " + std::to_string(s));
      model.solveStep();
      if (is_newton(model)) {
        auto & nls = model.getNonLinearSolver();
        its.push_back(Int(nls.get("nb_iterations")));
        errs.push_back(Real(nls.get("error")));
      }
      energies.push_back(model.getEnergy("thermal"));
    }
    stage.push_back("record");
    add_array(w, "temperature", T, "final temperature");
    if (rate != nullptr) {
      add_array(w, "temperature_rate", *rate, "final temperature rate");
    }
    add_array(w, "internal_heat_rate", model.getInternalHeatRate(), "final internal flow");
    add_vec(w, "energy/thermal_per_step", energies, "getEnergy(thermal) after each step");
    if (not its.empty()) {
      add_ivec(w, "newton/nb_iterations", its, "per step");
      add_vec(w, "newton/error", errs, "per step");
    }
    record_aij(w, "K", model, "K");
    record_aij(w, "M", model, "M");
    record_lumped(w, model);
    stage.push_back("done");
  } catch (std::exception & e) {
    p["exception"] = std::string(e.what());
    failed = true;
  }
  Json sj = Json::array();
  for (auto & s : stage) {
    sj.push(s);
  }
  p["stages"] = sj;
  set_nan_notes(w, c.nan_why.empty() ? std::string("unexpected: no reason recorded") : c.nan_why);
  w.finish();
  std::cout << "  " << cid << (failed ? " (exception)" : "") << std::endl;
}

/* -------------------------------------------------------------------------- */
/* UD-283: cohesive elements and the stable time step                         */
/* -------------------------------------------------------------------------- */
void ud283_cohesive_dt() {
  const std::string cid = "model/diffusion/v102_ud283_cohesive_dt";
  if (not wanted("ud283_cohesive_dt")) {
    return;
  }
  auto mesh_file = g_up / "test/test_model/patch_tests/data/_triangle_3.msh";
  auto dat = fs::temp_directory_path() / "v102_cohesive.dat";
  write_file(dat, heat_dat("[[2.0, 0.0], [0.0, 2.0]]", 2.5, 3., 0., 0.));
  g_nans.clear();
  ap::FixtureWriter w(g_root, cid,
                      "UD-283: getStableTimeStep before / after a _cohesive_2d_4 element "
                      "is appended to the mesh (review V102)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("diffusion");
  w.add_input_file("mesh", mesh_file, "upstream:test/test_model/patch_tests/data/_triangle_3.msh");
  w.add_input_file("material", dat, "generated:v102_probe.cc");
  auto & p = w.parameters();
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(dat.string());
  HeatTransferModel model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
  const Real before = model.getStableTimeStep();
  {
    MeshAccessor acc(mesh);
    auto & conn = acc.getConnectivity(_cohesive_2d_4);
    Vector<Idx> c{0, 1, 3, 2};
    conn.push_back(c);
  }
  Real after = -1.;
  try {
    after = model.getStableTimeStep();
  } catch (std::exception & e) {
    p["exception"] = std::string(e.what());
  }
  Json types = Json::array();
  for (auto type : mesh.elementTypes(2, _not_ghost)) {
    types.push(std::to_string(type));
  }
  p["types_dim2_not_ghost"] = types;
  add_vec(w, "stable_time_step", {before, after}, "before, after appending _cohesive_2d_4");
  set_nan_notes(w, "unexpected: no reason recorded");
  w.finish();
  std::cout << "  " << cid << " before " << before << " after " << after << std::endl;
}

void ud283_cohesive_init() {
  const std::string cid = "model/diffusion/v102_ud283_cohesive_init";
  if (not wanted("ud283_cohesive_init")) {
    return;
  }
  auto mesh_file = g_up / "test/test_model/patch_tests/data/_triangle_3.msh";
  auto dat = fs::temp_directory_path() / "v102_cohesive2.dat";
  write_file(dat, heat_dat("[[2.0, 0.0], [0.0, 2.0]]", 2.5, 3., 0., 0.));
  g_nans.clear();
  ap::FixtureWriter w(g_root, cid,
                      "UD-283: HeatTransferModel on a mesh that holds a _cohesive_2d_4 "
                      "element before the model exists (review V102)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("diffusion");
  // Inputs recorded (the cohesive element {0, 1, 3, 2} is
  // appended in memory to the upstream mesh, as below)
  w.add_input_file("mesh", mesh_file, "upstream:test/test_model/patch_tests/data/_triangle_3.msh");
  w.add_input_file("material", dat, "generated:v102_probe.cc");
  auto & p = w.parameters();
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  {
    MeshAccessor acc(mesh);
    auto & conn = acc.getConnectivity(_cohesive_2d_4);
    Vector<Idx> c{0, 1, 3, 2};
    conn.push_back(c);
  }
  getStaticParser().parse(dat.string());
  std::vector<std::string> stage;
  try {
    stage.push_back("construct");
    HeatTransferModel model(mesh);
    stage.push_back("initFull");
    model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
    stage.push_back("getStableTimeStep");
    Real dt = model.getStableTimeStep();
    p["stable_time_step"] = dt;
    stage.push_back("solveStep");
    model.setTimeStep(1e-3);
    model.solveStep();
    stage.push_back("done");
  } catch (std::exception & e) {
    p["exception"] = std::string(e.what());
  }
  Json sj = Json::array();
  for (auto & s : stage) {
    sj.push(s);
  }
  p["stages"] = sj;
  set_nan_notes(w, "unexpected: no reason recorded");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

} // namespace

/* -------------------------------------------------------------------------- */
int main(int argc, char * argv[]) {
  // x86_64 pin (D-050): aarch64 cache sizes for Eigen
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (argc < 3) {
    std::cerr << "usage: v102_probe <fixtures_root> <upstream_src_root>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    ud287_bar();

    const std::string aniso2 = "[[3.0, 1.2], [1.2, 1.0]]";
    const std::string aniso3 = "[[2.0, 0.4, 0.2], [0.4, 1.5, 0.3], [0.2, 0.3, 1.0]]";
    auto lr_bc = [](Real xmax) {
      return [xmax](const Vector<Real> & x, Real & T, bool & b, Real & r) {
        r = 0.;
        b = false;
        T = 50. + 10. * x(1);
        if (x(0) < 1e-12) {
          b = true;
          T = 100.;
        } else if (x(0) > xmax - 1e-12) {
          b = true;
          T = 0.;
        }
      };
    };
    const std::vector<std::tuple<AnalysisMethod, std::string, Real, Int>> analyses = {
        {_static, "static", 0., 1},
        {_explicit_lumped_mass, "explicit", 0.5, 20},
        {_implicit_dynamic, "implicit", 10., 10}};
    for (auto && [m, a, f, s] : analyses) {
      CaseSetup c{"aniso2d_" + a,
                  "mixed quadrangle_4 / triangle_3 mesh, full anisotropic conductivity (" + a +
                      ")",
                  2, mixed2d_mesh(), "", heat_dat(aniso2, 2., 1.5, 0., 0.), m, f, 0., s,
                  lr_bc(2.)};
      run_case(c);
    }
    for (auto && [m, a, f, s] : analyses) {
      if (m == _static) {
        continue;
      }
      CaseSetup c{"aniso3d_" + a,
                  "mixed hexahedron_8 / pentahedron_6 mesh, full 3x3 anisotropic conductivity (" +
                      a + ")",
                  3, mixed3d_mesh(), "", heat_dat(aniso3, 1.5, 2., 0., 0.), m, f, 0.,
                  s / 2, lr_bc(2.)};
      run_case(c);
    }
    auto rate_init = [](const Vector<Real> & x, Real & T, bool & b, Real & r) {
      T = 1. + x(0) + 2. * x(1);
      b = x(0) < 1e-12;
      r = b ? 0. : 0.5 + 0.25 * x(0) - 0.75 * x(1) * x(1);
    };
    for (auto && [m, a, f] : std::vector<std::tuple<AnalysisMethod, std::string, Real>>{
             {_explicit_lumped_mass, "explicit", 0.5}, {_implicit_dynamic, "implicit", 5.}}) {
      CaseSetup c{"rate0_" + a,
                  "non-zero non-consistent initial temperature rate (" + a + ")",
                  2, "", "_quadrangle_4.msh",
                  heat_dat("[[1.5, 0.2], [0.2, 1.0]]", 2., 1., 0., 0.), m, f, 0., 5, rate_init};
      c.set_rate = true;
      run_case(c);
    }
    auto kink_init = [](const Vector<Real> & x, Real & T, bool & b, Real & r) {
      r = 0.;
      T = -10. + 45. * x(0); // patch meshes span [0, 1]
      b = x(0) < 1e-12 || x(0) > 1. - 1e-12;
    };
    for (auto && [m, a, f, s] : std::vector<std::tuple<AnalysisMethod, std::string, Real, Int>>{
             {_explicit_lumped_mass, "explicit", 0.2, 10}, {_implicit_dynamic, "implicit", 0.5, 5}}) {
      CaseSetup c{"kink_" + a,
                  "conductivity 1.2 + 0.1 (T - 20) on every entry: the diagonal crosses zero at "
                  "T = 8, T from -10 to 35 (" + a + ")",
                  2, "", "_triangle_3.msh",
                  heat_dat("[[1.2, 0.0], [0.0, 1.2]]", 1., 1., 0.1, 20.), m, f, 0., s, kink_init};
      run_case(c);
    }
    auto zr_init = [](const Vector<Real> & x, Real & T, bool & b, Real & r) {
      r = 0.;
      T = 20.;
      if (x(0) < 1e-12) {
        b = true;
        T = 10.;
      } else if (x(0) > 1. - 1e-12) {
        b = true;
        T = 30.;
      } else {
        b = false;
      }
    };
    for (auto && [m, a, dt, s] : std::vector<std::tuple<AnalysisMethod, std::string, Real, Int>>{
             {_static, "static", 0., 1},
             {_implicit_dynamic, "implicit", 1e-2, 3},
             {_explicit_lumped_mass, "explicit", 1e-3, 2}}) {
      CaseSetup c{"zerorho_" + a, "density = 0 (" + a + ")", 2, "", "_quadrangle_4.msh",
                  heat_dat("[[1.0, 0.0], [0.0, 1.0]]", 0., 3., 0., 0.), m, 0., dt, s, zr_init};
      c.nan_why = "density 0: the explicit lumped solve divides the residual by a zero lumped "
                  "capacity (inf, then inf - inf)";
      run_case(c);
    }
    {
      CaseSetup c{"ud281_lumped",
                  "UD-281: explicit lumped solve after assembleCapacityMatrix()",
                  2, "", "_triangle_3.msh",
                  heat_dat("[[1.0, 0.0], [0.0, 1.0]]", 2., 1., 0., 0.),
                  _explicit_lumped_mass, 0.5, 0., 3, zr_init};
      c.consistent_first = true;
      c.nan_why = "UD-281: after assembleCapacityMatrix() the lumped capacity is never "
                  "assembled and the explicit solve divides by zero";
      run_case(c);
    }
    ud283_cohesive_dt();
    ud283_cohesive_init();
  } catch (std::exception & e) {
    std::cerr << "v102_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
