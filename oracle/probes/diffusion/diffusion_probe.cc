// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs HeatTransferModel scenarios with the Eigen sparse
// solver (every solve reproducible by the port's bit-identical SparseLU
// translation, D-008) and records the model state for the Rust differential
// tests (fixtures/model/diffusion/*).
//
// Cases (fixture area model, case ids model/diffusion/<case>, format v1):
//   patch_<analysis>_<type>
//       intent of the (never built, UD-014/UD-289) upstream heat-transfer
//       patch tests test/test_model/patch_tests/
//       patch_test_linear_heat_transfer_{static,implicit,explicit}.cc with
//       patch_test_linear_heat_transfer_fixture.hh and
//       patch_test_linear_fixture.hh, for every regular element type:
//       buildFacets + createBoundaryGroupFromGeometry, HeatTransferModel with
//       id = type name, heat_transfer_input.dat, nodes of every element
//       group blocked at the affine field T = 0.01 + 0.02 x + 0.03 y +
//       0.04 z; Newton-Raphson max_iterations 3 / threshold 1e-6 /
//       solution criterion; dynamic: setTimeStep(0.8 dt_stable) then
//       setTimeStep(0.5 dt_stable) (the heat fixture overrides the parent's
//       step), every temperature set to the affine field, 100 steps; static:
//       max_iterations 2, threshold 2e-4, residual criterion, one step.
//   nonlinear_<analysis>_<type>
//       probe-defined: anisotropic conductivity with off-diagonal terms,
//       conductivity_variation = 0.05, temperature_reference = 20
//       (temperature-dependent diffusivity, added to every entry upstream),
//       boundary nodes blocked at a quadratic field, interior at 20;
//       static (Newton-Raphson, 50 iterations, 1e-11, solution criterion;
//       upstream K ignores dk/dT, so Newton converges linearly),
//       implicit (20 steps, dt = 25 dt_stable) and explicit (50 steps,
//       dt = 0.5 dt_stable).
//   square_<analysis>, cube_explicit
//       the upstream examples examples/c++/diffusion_model/
//       heat_diffusion_{static_2d,dynamics_2d,dynamics_3d}.cc on the frozen
//       irregular second-order meshes (square.msh: _triangle_6, cube.msh:
//       _tetrahedron_10), shortened (explicit 40 / 10 steps, implicit 10
//       steps with dt = 1e-3 * ... see below).
//   two_laws_mixed
//       probe-defined mixed _triangle_3 / _quadrangle_4 mesh (written by the
//       probe), two heat_diffusion laws selected by physical name: upstream
//       DiffusionLaw::assembleInternalFlow / assembleDiffusivityMatrix
//       `return` on the first empty per-type filter (UD-276), so the second
//       law's quadrangles are never assembled. Explicit steps and K.
//
// Usage: diffusion_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: DIFF_PROBE_CASES=<id>[,<id>...] restricts the run.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <constitutive_law_selector.hh>
#include <constitutive_law_selector_tmpl.hh>
#include <element_group.hh>
#include <heat_transfer_model.hh>
#include <mesh.hh>
#include <mesh_utils.hh>
#include <non_linear_solver.hh>
#include <non_linear_solver_newton_raphson.hh>
#include <sparse_matrix_aij.hh>

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "diffusion_probe";
constexpr const char * kProbeVersion = "1";

using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up, g_frozen;

bool wanted(const std::string & cid) {
  const char * env = std::getenv("DIFF_PROBE_CASES");
  if (env == nullptr || *env == '\0') {
    return true;
  }
  const std::string prefix = "model/diffusion/";
  const auto id = cid.rfind(prefix, 0) == 0 ? cid.substr(prefix.size()) : cid;
  std::stringstream list(env);
  std::string item;
  while (std::getline(list, item, ',')) {
    if (item == id) {
      return true;
    }
  }
  return false;
}

/* -------------------------------------------------------------------------- */
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

std::string digest(const std::vector<const Array<Real> *> & arrays) {
  ap::Sha256 h;
  for (const auto * a : arrays) {
    h.update(a->data(), a->size() * a->getNbComponent() * sizeof(double));
  }
  return h.hex();
}

std::string digest_vec(const std::vector<double> & v) {
  ap::Sha256 h;
  h.update(v.data(), v.size() * sizeof(double));
  return h.hex();
}

void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a))
      .description(desc);
}

void add_bools(ap::FixtureWriter & w, const std::string & name, const Array<bool> & a,
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

/// COO of an AIJ matrix (0-based rows/cols, storage order); `full = false`
/// records only sizes and digests.
void record_aij(ap::FixtureWriter & w, const std::string & prefix, Model & model, const ID & id,
                bool full) {
  if (not model.getDOFManager().hasMatrix(id)) {
    w.parameters()[prefix + "/exists"] = false;
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
  auto & p = w.parameters();
  p[prefix + "/exists"] = true;
  p[prefix + "/symmetric"] = A.getMatrixType() == _symmetric;
  p[prefix + "/size"] = static_cast<long long>(A.size());
  p[prefix + "/nnz"] = static_cast<long long>(nnz);
  {
    ap::Sha256 h;
    h.update(rows.data(), rows.size() * sizeof(I64));
    h.update(cols.data(), cols.size() * sizeof(I64));
    p[prefix + "/coords_sha256"] = h.hex();
    p[prefix + "/values_sha256"] = digest_vec(vals);
  }
  if (not full) {
    return;
  }
  add_ivec(w, prefix + "/rows", rows, "SparseMatrixAIJ::getIRN - 1 (storage order)");
  add_ivec(w, prefix + "/cols", cols, "SparseMatrixAIJ::getJCN - 1 (storage order)");
  add_vec(w, prefix + "/values", vals, "SparseMatrixAIJ::getA (storage order)");
}

/// Lumped matrix M per DOF (DOFManager::getLumpedMatrixPerDOFs).
void record_lumped(ap::FixtureWriter & w, HeatTransferModel & model, bool full) {
  auto & dm = model.getDOFManager();
  if (not dm.hasLumpedMatrix("M")) {
    w.parameters()["lumped_M/exists"] = false;
    return;
  }
  Array<Real> m(0, 1);
  m.resize(model.getTemperature().size());
  m.zero();
  dm.getLumpedMatrixPerDOFs("temperature", "M", m);
  w.parameters()["lumped_M/exists"] = true;
  w.parameters()["lumped_M/sha256"] = digest({&m});
  Real s = 0.;
  for (auto v : m) {
    s += v;
  }
  w.parameters()["lumped_M/sequential_sum"] = s;
  if (full) {
    add_array(w, "lumped_M", m, "DOFManager::getLumpedMatrixPerDOFs(temperature, M)");
  }
}

/// Raw internals of law 0 (∇u, D∇u, diffusivity) and its element filter.
void record_internals(ap::FixtureWriter & w, HeatTransferModel & model, Int law_index,
                      const std::string & prefix, bool full) {
  auto dim = model.getSpatialDimension();
  auto & law = model.getConstitutiveLaw(law_index);
  const auto & cl = static_cast<const DiffusionLaw &>(law);
  for (auto type : cl.getElementFilter().elementTypes(dim, _not_ghost)) {
    auto name = std::to_string(type);
    for (const auto * internal : {"∇u", "D∇u", "diffusivity"}) {
      const auto & a = law.getArray<Real>(internal, type);
      const std::string key = prefix + internal + "/" + name;
      if (full) {
        add_array(w, key, a,
                  std::string("law internal ") + internal +
                      " per quadrature point (raw tuples; diffusivity column-major dim x dim)");
      } else {
        w.parameters()[key + "/sha256"] = digest({&a});
      }
    }
    const auto & f = cl.getElementFilter(type);
    std::vector<I64> fv(f.data(), f.data() + f.size());
    if (full) {
      add_ivec(w, prefix + "filter/" + name, fv, "element filter of the law");
    } else {
      ap::Sha256 h;
      h.update(fv.data(), fv.size() * sizeof(I64));
      w.parameters()[prefix + "filter/" + name + "/sha256"] = h.hex();
    }
  }
}

/// Mesh after buildFacets + createBoundaryGroupFromGeometry (connectivities
/// of every dimension and every element group).
void record_mesh(ap::FixtureWriter & w, const Mesh & mesh) {
  auto & p = w.parameters();
  Json types = Json::array();
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    const auto & c = mesh.getConnectivity(type);
    std::vector<I64> v(c.size() * c.getNbComponent());
    for (std::size_t i = 0; i < v.size(); ++i) {
      v[i] = c.data()[i];
    }
    auto name = std::to_string(type);
    types.push(name);
    w.add("mesh/connectivity/" + name,
          {static_cast<std::size_t>(c.size()), static_cast<std::size_t>(c.getNbComponent())},
          {"element", "node_local"}, "elemental", v)
        .element_type(name)
        .description("connectivity after buildFacets (facets appended by upstream)");
  }
  p["mesh_types"] = types;
  Json groups = Json::array();
  for (auto & g : mesh.iterateElementGroups()) {
    Json gj = Json::object();
    gj["name"] = g.getName();
    gj["dimension"] = static_cast<long long>(g.getDimension());
    Json gt = Json::array();
    for (auto type : g.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
      const auto & e = g.getElements(type);
      std::vector<I64> v(e.data(), e.data() + e.size());
      auto name = std::to_string(type);
      gt.push(name);
      add_ivec(w, "group/" + g.getName() + "/elements/" + name, v, "element group elements");
    }
    gj["types"] = gt;
    groups.push(gj);
    std::vector<I64> nodes;
    for (auto n : g.getNodeGroup()) {
      nodes.push_back(n);
    }
    add_ivec(w, "group/" + g.getName() + "/nodes", nodes, "node group (iteration order)");
  }
  p["groups"] = groups;
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

void set_newton(HeatTransferModel & model, Int max_it, Real threshold,
                SolveConvergenceCriteria crit) {
  auto & solver = model.getNonLinearSolver();
  if (aka::is_of_type<NonLinearSolverNewtonRaphson>(solver)) {
    solver.set("max_iterations", max_it);
    solver.set("threshold", threshold);
    solver.set("convergence_type", crit);
  }
}

bool is_newton(HeatTransferModel & model) {
  return aka::is_of_type<NonLinearSolverNewtonRaphson>(model.getNonLinearSolver());
}

/// Thermal energy per element of every not-ghost element of the model
/// dimension (only for single-quadrature-point types: upstream's
/// per-element integrate writes nb_quadrature_points values into a single
/// Real on the stack, UD-277).
void record_element_energies(ap::FixtureWriter & w, HeatTransferModel & model) {
  auto & mesh = model.getMesh();
  auto dim = model.getSpatialDimension();
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
    if (model.getFEEngine().getNbIntegrationPoints(type) != 1) {
      continue;
    }
    std::vector<double> e;
    for (Int el = 0; el < mesh.getNbElement(type); ++el) {
      e.push_back(model.getEnergy("thermal", Element{type, el, _not_ghost}));
    }
    add_vec(w, "energy/thermal_by_element/" + std::to_string(type), e,
            "getEnergy(\"thermal\", element) for every element (nb_quadrature_points == 1)");
  }
}

/// Common final-state record.
void record_state(ap::FixtureWriter & w, HeatTransferModel & model, bool dynamic,
                  bool full_arrays, bool full_matrices) {
  add_array(w, "temperature", model.getTemperature(), "final temperature");
  // (DOFManager::hasDOFsDerivatives(id, 1) is false for an order-1 family,
  // UD-012: decided by the analysis instead)
  if (dynamic) {
    add_array(w, "temperature_rate", model.getTemperatureRate(), "final temperature rate");
  }
  add_array(w, "internal_heat_rate", model.getInternalHeatRate(), "final internal flow");
  add_vec(w, "energy/thermal", {model.getEnergy("thermal")}, "getEnergy(\"thermal\")");
  record_internals(w, model, 0, "law0/", full_arrays);
  record_aij(w, "K", model, "K", full_matrices);
  record_aij(w, "M", model, "M", full_matrices);
  record_aij(w, "J", model, "J", false);
  record_lumped(w, model, full_arrays);
}

/* -------------------------------------------------------------------------- */
const Real kAlpha[4] = {0.01, 0.02, 0.03, 0.04};

template <class V> Real linear_field(const V & X, Int dim) {
  Real t = kAlpha[0];
  for (Int j = 0; j < dim; ++j) {
    t += kAlpha[j + 1] * X(j);
  }
  return t;
}

void patch_case(ElementType type, AnalysisMethod method, const std::string & analysis) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  const std::string cid = "model/diffusion/patch_" + analysis + "_" + tname.substr(1);
  if (not wanted(cid)) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  MeshUtils::buildFacets(mesh);
  mesh.createBoundaryGroupFromGeometry();
  HeatTransferModel model(mesh, _all_dimensions, tname);
  getStaticParser().parse((data / "heat_transfer_input.dat").string());
  model.initFull(_analysis_method = method, _solver_options = eigen_options());
  // applyBC (parent): block every node of every element group
  auto & blocked = model.getBlockedDOFs();
  auto & T = model.getTemperature();
  const auto & X = mesh.getNodes();
  for (auto & eg : mesh.iterateElementGroups()) {
    for (const auto & node : eg.getNodeGroup()) {
      blocked(node) = true;
    }
  }
  // applyBConDOFs(temperature)
  for (auto & eg : mesh.iterateElementGroups()) {
    for (const auto & node : eg.getNodeGroup()) {
      T(node) = linear_field(make_view(X, dim).begin()[node], dim);
    }
  }
  set_newton(model, 3, 1e-6, SolveConvergenceCriteria::_solution);
  Real stable = 0., dt = 0.;
  if (method != _static) {
    stable = model.getStableTimeStep();
    model.setTimeStep(0.8 * stable);
    stable = model.getStableTimeStep();
    dt = 0.5 * stable;
    model.setTimeStep(dt);
  }
  ap::FixtureWriter w(g_root, cid,
                      "heat-transfer patch test (" + analysis + ", " + tname +
                          ") of test/test_model/patch_tests (intent; Eigen sparse solver)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("diffusion");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", data / "heat_transfer_input.dat",
                   "upstream:test/test_model/patch_tests/data/heat_transfer_input.dat");
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  p["model_id"] = tname;
  p["analysis"] = analysis;
  p["sparse_solver_type"] = "eigen";
  p["newton"] = is_newton(model);
  add_bools(w, "blocked_dofs", blocked, "blocked flags after applyBC");
  add_array(w, "initial_temperature", T, "temperature after applyBC (affine on boundary)");
  std::vector<std::string> digests;
  std::vector<I64> iterations;
  std::vector<double> errors, energies;
  auto & solver = model.getNonLinearSolver();
  if (method == _static) {
    set_newton(model, 2, 2e-4, SolveConvergenceCriteria::_residual);
    p["newton_max_iterations"] = 2LL;
    p["newton_threshold"] = 2e-4;
    p["newton_convergence_type"] = "residual";
    model.solveStep();
    iterations.push_back(Int(solver.get("nb_iterations")));
    errors.push_back(Real(solver.get("error")));
  } else {
    p["newton_max_iterations"] = 3LL;
    p["newton_threshold"] = 1e-6;
    p["newton_convergence_type"] = "solution";
    p["stable_time_step"] = stable;
    p["time_step"] = dt;
    add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), 0.5 * getStableTimeStep()");
    for (auto && [Xn, t] : zip(make_view(X, dim), T)) {
      t = linear_field(Xn, dim);
    }
    add_array(w, "initial_temperature_all", T, "temperature after setLinearDOF on every node");
    for (Int step = 0; step < 100; ++step) {
      model.solveStep();
      digests.push_back(digest({&model.getTemperature(), &model.getTemperatureRate(),
                                &model.getInternalHeatRate()}));
      if (is_newton(model)) {
        iterations.push_back(Int(solver.get("nb_iterations")));
        errors.push_back(Real(solver.get("error")));
      }
      if (step % 10 == 9) {
        energies.push_back(model.getEnergy("thermal"));
      }
    }
    Json dj = Json::array();
    for (auto & d : digests) {
      dj.push(d);
    }
    p["step_digests"] = dj;
    p["step_digests_definition"] =
        "SHA-256 of temperature || temperature_rate || internal_heat_rate (binary64 LE) "
        "after each solveStep";
    add_vec(w, "energy/thermal_every_10", energies, "getEnergy(\"thermal\") after steps 10, 20, ...");
  }
  if (not iterations.empty()) {
    add_ivec(w, "newton/nb_iterations", iterations, "nb_iterations per solveStep");
    add_vec(w, "newton/error", errors, "error per solveStep");
  }
  record_state(w, model, method != _static, true, true);
  record_element_energies(w, model);
  // HeatTransferModel aliases / conductivity read-back (checkAll's C)
  auto & cl = model.getConstitutiveLaw(0);
  Matrix<Real> C = cl.get("conductivity");
  std::vector<double> cv(C.data(), C.data() + C.size());
  add_vec(w, "law0/conductivity", cv, "getConstitutiveLaw(0).get(\"conductivity\") (column-major)");
  p["law0/conductivity_rows"] = static_cast<long long>(C.rows());
  record_mesh(w, mesh);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
const char * kNonlinearDat[4] = {
    "",
    "model heat_transfer_model [\n"
    "  constitutive_law heat_diffusion [\n"
    "    name = nl\n"
    "    density = 2.5\n"
    "    capacity = 3\n"
    "    conductivity = [[2.0]]\n"
    "    conductivity_variation = 0.05\n"
    "    temperature_reference = 20\n"
    "  ]\n"
    "]\n",
    "model heat_transfer_model [\n"
    "  constitutive_law heat_diffusion [\n"
    "    name = nl\n"
    "    density = 2.5\n"
    "    capacity = 3\n"
    "    conductivity = [[2.0, 0.5], [0.3, 3.0]]\n"
    "    conductivity_variation = 0.05\n"
    "    temperature_reference = 20\n"
    "  ]\n"
    "]\n",
    "model heat_transfer_model [\n"
    "  constitutive_law heat_diffusion [\n"
    "    name = nl\n"
    "    density = 2.5\n"
    "    capacity = 3\n"
    "    conductivity = [[2.0, 0.5, 0.1], [0.3, 3.0, 0.2], [0.1, 0.4, 1.5]]\n"
    "    conductivity_variation = 0.05\n"
    "    temperature_reference = 20\n"
    "  ]\n"
    "]\n"};

template <class V> Real quadratic_field(const V & X, Int dim) {
  // 20 + 30 x^2 + 10 x y - 5 y + 7 z^2 (sequential sums, port reproduces)
  Real x = X(0);
  Real y = dim > 1 ? X(1) : 0.;
  Real z = dim > 2 ? X(2) : 0.;
  Real t = 20.;
  t += 30. * x * x;
  t += 10. * x * y;
  t -= 5. * y;
  t += 7. * z * z;
  return t;
}

void nonlinear_case(ElementType type, AnalysisMethod method, const std::string & analysis) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  const std::string cid = "model/diffusion/nonlinear_" + analysis + "_" + tname.substr(1);
  if (not wanted(cid)) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  auto tmp = fs::temp_directory_path() / "diffusion_probe_nonlinear.dat";
  {
    std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
    f << kNonlinearDat[dim];
  }
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  MeshUtils::buildFacets(mesh);
  mesh.createBoundaryGroupFromGeometry();
  HeatTransferModel model(mesh, _all_dimensions, "heat_transfer");
  getStaticParser().parse(tmp.string());
  model.initFull(_analysis_method = method, _solver_options = eigen_options());
  const Real stable = model.getStableTimeStep();
  auto & blocked = model.getBlockedDOFs();
  auto & T = model.getTemperature();
  const auto & X = mesh.getNodes();
  for (auto && t : T) {
    t = 20.;
  }
  for (auto & eg : mesh.iterateElementGroups()) {
    for (const auto & node : eg.getNodeGroup()) {
      blocked(node) = true;
      T(node) = quadratic_field(make_view(X, dim).begin()[node], dim);
    }
  }
  Real dt = 0.;
  Int steps = 1;
  if (method == _static) {
    set_newton(model, 50, 1e-11, SolveConvergenceCriteria::_solution);
  } else if (method == _implicit_dynamic) {
    set_newton(model, 50, 1e-11, SolveConvergenceCriteria::_solution);
    dt = 25. * stable;
    steps = 20;
    model.setTimeStep(dt);
  } else {
    dt = 0.5 * stable;
    steps = 50;
    model.setTimeStep(dt);
  }
  ap::FixtureWriter w(g_root, cid,
                      "temperature-dependent anisotropic conductivity (" + analysis + ", " +
                          tname + "), probe-defined, Eigen sparse solver",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("diffusion");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", tmp, "generated:diffusion_probe.cc");
  fs::remove(tmp);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  p["model_id"] = "heat_transfer";
  p["analysis"] = analysis;
  p["steps"] = static_cast<long long>(steps);
  p["newton"] = is_newton(model);
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep() after initFull, time step used");
  add_bools(w, "blocked_dofs", blocked, "blocked flags");
  add_array(w, "initial_temperature", T, "initial temperature");
  auto & solver = model.getNonLinearSolver();
  std::vector<std::string> digests;
  std::vector<I64> iterations;
  std::vector<double> errors, energies;
  for (Int s = 0; s < steps; ++s) {
    model.solveStep();
    if (method != _static) {
      digests.push_back(digest({&model.getTemperature(), &model.getTemperatureRate(),
                                &model.getInternalHeatRate()}));
    }
    if (is_newton(model)) {
      iterations.push_back(Int(solver.get("nb_iterations")));
      errors.push_back(Real(solver.get("error")));
    }
    energies.push_back(model.getEnergy("thermal"));
  }
  if (not digests.empty()) {
    Json dj = Json::array();
    for (auto & d : digests) {
      dj.push(d);
    }
    p["step_digests"] = dj;
    p["step_digests_definition"] =
        "SHA-256 of temperature || temperature_rate || internal_heat_rate (binary64 LE) "
        "after each solveStep";
  }
  add_vec(w, "energy/thermal_per_step", energies, "getEnergy(\"thermal\") after each step");
  if (not iterations.empty()) {
    add_ivec(w, "newton/nb_iterations", iterations, "nb_iterations per solveStep");
    add_vec(w, "newton/error", errors, "error per solveStep");
  }
  record_state(w, model, method != _static, true, true);
  record_element_energies(w, model);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/// The upstream examples on the frozen irregular meshes.
void example_case(const std::string & which) {
  const std::string cid = "model/diffusion/" + which;
  if (not wanted(cid)) {
    return;
  }
  const bool cube = which == "cube_explicit";
  const Int dim = cube ? 3 : 2;
  const auto ex = g_up / "examples/c++/diffusion_model";
  const auto mesh_file = g_frozen / (std::string("examples/c++/diffusion_model/") +
                                     (cube ? "cube.msh" : "square.msh"));
  getStaticParser().parse((ex / "material.dat").string());
  Mesh mesh(dim);
  mesh.read(mesh_file.string());
  HeatTransferModel model(mesh);
  AnalysisMethod method = _explicit_lumped_mass;
  if (which == "square_static") {
    method = _static;
  } else if (which == "square_implicit") {
    method = _implicit_dynamic;
  }
  model.initFull(_analysis_method = method, _solver_options = eigen_options());
  const Real stable = model.getStableTimeStep();
  Real dt = 0.;
  Int steps = 1;
  if (method == _explicit_lumped_mass) {
    dt = stable * 0.8;
    steps = cube ? 10 : 40;
    model.setTimeStep(dt);
  } else if (method == _implicit_dynamic) {
    dt = stable * 50.;
    steps = 10;
    model.setTimeStep(dt);
  }
  const auto & nodes = mesh.getNodes();
  auto & boundary = model.getBlockedDOFs();
  auto & temperature = model.getTemperature();
  const double length = 1.;
  const auto nb_nodes = mesh.getNbNodes();
  for (Int i = 0; i < nb_nodes; ++i) {
    temperature(i) = 100.;
    if (which == "square_static") {
      // heat_diffusion_static_2d.cc
      Real dx = nodes(i, 0);
      Real dy = nodes(i, 1);
      Vector<Real> dX = {dx, dy};
      dX.array() -= length / 4.;
      Real d = dX.norm();
      if (d < 0.1) {
        boundary(i) = true;
        temperature(i) = 300.;
      }
      if (std::abs(dx) < 1e-4 || std::abs(dy) < 1e-4) {
        boundary(i) = true;
      }
      if (std::abs(dx - length) < 1e-4 || std::abs(dy - length) < 1e-4) {
        boundary(i) = true;
      }
      continue;
    }
    Real dx = 0., dy = 0., dz = 0.;
    if (cube) {
      // heat_diffusion_dynamics_3d.cc
      dx = nodes(i, 0) - length / 2.;
      dy = nodes(i, 1) - length / 2.;
      dz = nodes(i, 2) - length / 2.;
    } else {
      // heat_diffusion_dynamics_2d.cc
      dx = nodes(i, 0) - length / 4.;
      dy = nodes(i, 1) - length / 4.;
    }
    Real d = sqrt(dx * dx + dy * dy + dz * dz);
    if (d < 0.1) {
      boundary(i) = true;
      temperature(i) = 300.;
    }
  }
  ap::FixtureWriter w(g_root, cid,
                      "upstream diffusion example on the frozen irregular mesh (" + which +
                          ", Eigen sparse solver)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("diffusion");
  // referenced, not copied (size policy): fixtures/meshes/<gmsh>/...
  w.add_input_reference("mesh",
                        std::string("frozen:examples/c++/diffusion_model/") +
                            (cube ? "cube.msh" : "square.msh"),
                        mesh_file);
  w.add_input_file("material", ex / "material.dat",
                   "upstream:examples/c++/diffusion_model/material.dat");
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["model_id"] = "heat_transfer";
  p["analysis"] = which;
  p["steps"] = static_cast<long long>(steps);
  p["newton"] = is_newton(model);
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
  add_bools(w, "blocked_dofs", boundary, "blocked flags");
  w.parameters()["initial_temperature/sha256"] = digest({&temperature});
  auto & solver = model.getNonLinearSolver();
  std::vector<std::string> digests;
  std::vector<I64> iterations;
  std::vector<double> errors, energies;
  for (Int s = 0; s < steps; ++s) {
    model.solveStep();
    if (method != _static) {
      digests.push_back(digest({&model.getTemperature(), &model.getTemperatureRate(),
                                &model.getInternalHeatRate()}));
    }
    if (is_newton(model)) {
      iterations.push_back(Int(solver.get("nb_iterations")));
      errors.push_back(Real(solver.get("error")));
    }
    energies.push_back(model.getEnergy("thermal"));
  }
  if (not digests.empty()) {
    Json dj = Json::array();
    for (auto & d : digests) {
      dj.push(d);
    }
    p["step_digests"] = dj;
    p["step_digests_definition"] =
        "SHA-256 of temperature || temperature_rate || internal_heat_rate (binary64 LE) "
        "after each solveStep";
  }
  add_vec(w, "energy/thermal_per_step", energies, "getEnergy(\"thermal\") after each step");
  if (not iterations.empty()) {
    add_ivec(w, "newton/nb_iterations", iterations, "nb_iterations per solveStep");
    add_vec(w, "newton/error", errors, "error per solveStep");
  }
  record_state(w, model, method != _static, false, false);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/// Two laws on a mixed _triangle_3 / _quadrangle_4 mesh (UD-276).
void two_laws_case() {
  const std::string cid = "model/diffusion/two_laws_mixed";
  if (not wanted(cid)) {
    return;
  }
  auto msh = fs::temp_directory_path() / "diffusion_probe_mixed.msh";
  {
    // 3 x 1 strip: quads on [0,2] x [0,1] (2 quads), triangles on [2,3] x
    // [0,1] (2 triangles), slightly perturbed interior nodes.
    std::ofstream f(msh, std::ios::binary | std::ios::trunc);
    f << "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n"
         "$PhysicalNames\n2\n2 1 \"quads\"\n2 2 \"tris\"\n$EndPhysicalNames\n"
         "$Nodes\n8\n"
         "1 0 0 0\n2 1.1 0 0\n3 2 0 0\n4 3 0 0\n"
         "5 0 1 0\n6 0.9 1 0\n7 2 1 0\n8 3 1 0\n$EndNodes\n"
         "$Elements\n4\n"
         "1 3 2 1 1 1 2 6 5\n"
         "2 3 2 1 1 2 3 7 6\n"
         "3 2 2 2 2 3 4 8\n"
         "4 2 2 2 2 3 8 7\n"
         "$EndElements\n";
  }
  auto dat = fs::temp_directory_path() / "diffusion_probe_two_laws.dat";
  {
    std::ofstream f(dat, std::ios::binary | std::ios::trunc);
    f << "model heat_transfer_model [\n"
         "  constitutive_law heat_diffusion [\n    name = quads\n    density = 2\n"
         "    capacity = 3\n    conductivity = [[4, 0], [0, 4]]\n  ]\n"
         "  constitutive_law heat_diffusion [\n    name = tris\n    density = 1\n"
         "    capacity = 5\n    conductivity = [[1, 0], [0, 2]]\n  ]\n"
         "]\n";
  }
  Mesh mesh(2);
  mesh.read(msh.string());
  HeatTransferModel model(mesh);
  getStaticParser().parse(dat.string());
  auto selector =
      std::make_shared<MeshDataConstitutiveLawSelector<std::string, DiffusionModel>>(
          "physical_names", model);
  model.setConstitutiveLawSelector(selector);
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.5 * stable;
  model.setTimeStep(dt);
  auto & T = model.getTemperature();
  auto & blocked = model.getBlockedDOFs();
  const auto & X = mesh.getNodes();
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    T(n) = 10. + 5. * X(n, 0) + 2. * X(n, 1) * X(n, 0);
  }
  blocked(0) = true;
  blocked(4) = true;
  ap::FixtureWriter w(g_root, cid,
                      "two heat_diffusion laws on a mixed triangle_3 / quadrangle_4 mesh "
                      "(UD-276: upstream returns on the first empty per-type filter)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("diffusion");
  w.add_input_file("mesh", msh, "generated:diffusion_probe.cc");
  w.add_input_file("material", dat, "generated:diffusion_probe.cc");
  fs::remove(msh);
  fs::remove(dat);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["model_id"] = "heat_transfer";
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
  add_array(w, "initial_temperature", T, "initial temperature");
  for (Int s = 0; s < 5; ++s) {
    model.solveStep();
  }
  add_array(w, "temperature", model.getTemperature(), "temperature after 5 explicit steps");
  add_array(w, "temperature_rate", model.getTemperatureRate(), "temperature rate");
  add_array(w, "internal_heat_rate", model.getInternalHeatRate(),
            "internal flow (law 'tris' never assembled upstream, UD-276)");
  record_lumped(w, model, true);
  // K through the static entry point (explicit model: create K first)
  if (not model.getDOFManager().hasMatrix("K")) {
    model.getDOFManager().getNewMatrix("K", _symmetric);
  }
  model.assembleDiffisivityMatrix();
  record_aij(w, "K", model, "K", true);
  try {
    p["energy/thermal"] = model.getEnergy("thermal");
  } catch (debug::Exception & e) {
    p["energy/thermal_exception"] = std::string(e.what());
  }
  for (Int l = 0; l < 2; ++l) {
    record_internals(w, model, l, "law" + std::to_string(l) + "/", true);
  }
  w.finish();
  std::cout << "  " << cid << std::endl;
}

} // namespace

/* -------------------------------------------------------------------------- */
int main(int argc, char * argv[]) {
  // x86_64 pin (D-050; with it the 56 cases
  // regenerate byte-identical on the x86_64 oracle): Eigen's cache sizes of
  // the aarch64 build (no CPUID query), whatever the host CPU
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (argc < 4) {
    std::cerr << "usage: diffusion_probe <fixtures_root> <upstream_src_root> "
                 "<frozen_meshes_root>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    const std::vector<ElementType> all = {
        _segment_2,    _segment_3,     _triangle_3,    _triangle_6,
        _quadrangle_4, _quadrangle_8,  _tetrahedron_4, _tetrahedron_10,
        _hexahedron_8, _hexahedron_20, _pentahedron_6, _pentahedron_15};
    const std::vector<std::tuple<AnalysisMethod, std::string>> analyses = {
        {_static, "static"}, {_explicit_lumped_mass, "explicit"}, {_implicit_dynamic, "implicit"}};
    for (auto type : all) {
      for (auto && [m, a] : analyses) {
        patch_case(type, m, a);
      }
    }
    for (auto type : {_segment_2, _triangle_3, _quadrangle_8, _tetrahedron_4, _hexahedron_8}) {
      for (auto && [m, a] : analyses) {
        nonlinear_case(type, m, a);
      }
    }
    for (const auto * which : {"square_static", "square_explicit", "square_implicit",
                               "cube_explicit"}) {
      example_case(which);
    }
    two_laws_case();
  } catch (std::exception & e) {
    std::cerr << "diffusion_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
