// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) for SMM-023: SolidMechanicsModel scenarios with an elastic material
// in **finite deformation** (finite_deformation = true, total-Lagrangian
// assembly of material.cc: assembleInternalForcesFiniteDeformation,
// assembleStiffnessMatrixNL / L2), run with the Eigen sparse solver so that
// every solve is reproducible by the port's bit-identical SparseLU
// translation (D-008), recorded for the Rust differential tests
// (fixtures/model/smm_fd/*).
//
// The upstream finite-deformation patch tests never exercise finite
// deformation (UD-210: plane strain / 3-D read a material file with
// finite_deformation = false, plane stress returns early), so the cases
// below are probe-defined, with the upstream meshes and fixtures:
//
//   static_patch_<type>[_plane_stress]
//       patch test (buildFacets + createBoundaryGroupFromGeometry, every
//       element-group node blocked at the affine field of
//       patch_test_linear_fixture.hh), static Newton-Raphson (solution
//       criterion, threshold 1e-10, max 10 iterations); material
//       elastic steel with finite_deformation = true (plane strain), or the
//       upstream material_check_stress_plane_stress_finite_deformation.dat
//       (plane stress, finite_deformation = true); then Material::onDump
//       (Cauchy stresses)
//   explicit_patch_<type>, implicit_patch_<type>
//       ExplicitFiniteDeformation / ImplicitFiniteDeformation of
//       patch_test_linear_elastic_{explicit,implicit}.cc with a genuinely
//       finite-deformation material: every node at the affine field, 100
//       steps, dt = 0.8 getStableTimeStep (Newton: solution criterion,
//       threshold 1e-6, max 3 iterations as the patch fixture)
//   dynamic_explicit_<type>, dynamic_implicit_<type>
//       x = xmin face blocked, non-affine initial displacement (finite
//       strains), explicit lumped mass: amplitude 0.1, 50 steps at
//       0.5 getStableTimeStep; implicit dynamic: amplitude 0.05, 10 steps of
//       1e-4 s (Newton: residual criterion, threshold 1e-9, max 30
//       iterations; amplitude 0.1 diverges on _tetrahedron_4/10)
//   loaded_static_<type>
//       x = xmin face blocked, nodal load (2e9, -1e9, 5e8) on every x = xmax
//       node, static Newton-Raphson (residual, 1e-9, max 30): large
//       deformation
//   large_rotation_tetrahedron_4
//       test/test_model/test_solid_mechanics_model/
//       test_solid_mechanics_model_material_large_rotation.cc (cube_3d_tet_4,
//       material_elastic.dat with finite_deformation = 1, implicit dynamic,
//       residual 1e-4, max 100 iterations; assembleMass, row sums through
//       the const M(i, j) (the test's non-const accessor aborts, UD-078)
//       and "mass centre" over the in-range DOFs (the test's loop reads 3x
//       past the end), one solveStep)
//   rotation_tetrahedron_4
//       test/test_model/test_solid_mechanics_model/test_materials/
//       test_finite_deformation.cc (1_tetrahedron.msh,
//       material_finite_deformation.dat, static, every node blocked at the
//       affine field, solveStep; then rotated by pi/4 about x, solveStep)
//   eigenstrain_stiffness_quadrangle_4
//       loaded_static_quadrangle_4 with eigen_grad_u, then
//       assembleStiffnessMatrix(true): upstream evaluates B_L1 of the L2
//       part at grad u (not grad u - eigen_grad_u, UD-208)
//   mixed_quad8_tri6_fd_fd, mixed_quad8_tri6_fd_fd_linear,
//   mixed_quad8_tri6_fd_lin
//       one quadrangle_8
//       (material a, finite deformation) and two triangle_6 (material b:
//       finite deformation, or small strain for _fd_lin) selected by
//       physical name, segment_3 boundary groups; left blocked, bottom y
//       blocked; two load steps (lambda = 0.5, 1) with f_ext zeroed and
//       FromSameDim traction on right / FromHigherDim stress on top
//       re-applied; static Newton-Raphson (solution criterion, threshold
//       1e-10, max 50), or the _linear non-linear solver (residual through
//       the AIJ mat-vec of K, whose COO order is then observable); every
//       material's filter has an empty type, which upstream's FD stiffness
//       still adds to K's profile (no empty-filter early return)
//
// Usage: smm_fd_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: SMM_PROBE_CASES=<id>[,<id>...] restricts the run to the
// listed case ids (without the model/smm_fd/ prefix; unset: every case).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <mesh_utils.hh>
#include <non_linear_solver.hh>
#include <non_linear_solver_newton_raphson.hh>
#include <solid_mechanics_model.hh>
#include <sparse_matrix_aij.hh>

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "smm_fd_probe";
constexpr const char * kProbeVersion = "1";
constexpr const char * kArea = "model/smm_fd/";

using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up, g_frozen;

bool wanted(const std::string & id) {
  const char * env = std::getenv("SMM_PROBE_CASES");
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

/// SHA-256 of the arrays' doubles, default NaNs canonicalised (D-046:
/// identical to the raw bytes on aarch64; x86_64 runs give the same digest).
std::string digest(const std::vector<const Array<Real> *> & arrays) {
  ap::Sha256 h;
  for (const auto * a : arrays) {
    ap::update_doubles_d046(h, a->data(),
                            static_cast<std::size_t>(a->size() * a->getNbComponent()));
  }
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

/// COO of an AIJ matrix (0-based rows/cols, storage order).
void record_aij(ap::FixtureWriter & w, const std::string & prefix, SolidMechanicsModel & model,
                const ID & id, bool full = true) {
  if (not model.getDOFManager().hasMatrix(id)) {
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
  w.parameters()[prefix + "/symmetric"] = A.getMatrixType() == _symmetric;
  w.parameters()[prefix + "/size"] = static_cast<long long>(A.size());
  w.parameters()[prefix + "/nnz"] = static_cast<long long>(nnz);
  {
    ap::Sha256 h;
    h.update(rows.data(), rows.size() * sizeof(I64));
    h.update(cols.data(), cols.size() * sizeof(I64));
    w.parameters()[prefix + "/coords_sha256"] = h.hex();
    ap::Sha256 hv;
    ap::update_doubles_d046(hv, vals.data(), vals.size()); // D-046, as digest()
    w.parameters()[prefix + "/values_sha256"] = hv.hex();
  }
  if (not full) {
    return;
  }
  add_ivec(w, prefix + "/rows", rows, "SparseMatrixAIJ::getIRN - 1 (storage order)");
  add_ivec(w, prefix + "/cols", cols, "SparseMatrixAIJ::getJCN - 1 (storage order)");
  add_vec(w, prefix + "/values", vals, "SparseMatrixAIJ::getA (storage order)");
}

/// Mesh after buildFacets: every connectivity and element group (as
/// smm_probe.cc record_mesh).
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

/// Internals of material 0 (raw column-major dim x dim tuples): stress,
/// grad_u, piola_kirchhoff_2, element filter.
void record_internals(ap::FixtureWriter & w, SolidMechanicsModel & model,
                      const std::string & prefix = "") {
  auto dim = model.getSpatialDimension();
  auto & mat = model.getMaterial(0);
  for (auto type : static_cast<const Material &>(mat).getElementFilter().elementTypes(dim, _not_ghost)) {
    auto name = std::to_string(type);
    add_array(w, prefix + "stress/" + name, mat.getStress(type),
              "material stress per quadrature point, raw column-major dim x dim tuples");
    add_array(w, prefix + "grad_u/" + name, mat.getGradU(type),
              "material grad_u per quadrature point, raw column-major dim x dim tuples");
    add_array(w, prefix + "piola_kirchhoff_2/" + name,
              mat.getArray<Real>("piola_kirchhoff_2", type),
              "second Piola-Kirchhoff stress per quadrature point, raw column-major tuples");
    const auto & cf = static_cast<const Material &>(mat).getElementFilter(type);
    std::vector<I64> f(cf.data(), cf.data() + cf.size());
    add_ivec(w, prefix + "filter/" + name, f, "element filter of the material");
  }
}

/// Material::onDump (finite deformation: computeAllCauchyStresses), then
/// the stress internals.
void record_cauchy(ap::FixtureWriter & w, SolidMechanicsModel & model) {
  auto dim = model.getSpatialDimension();
  auto & mat = model.getMaterial(0);
  mat.onDump();
  for (auto type : static_cast<const Material &>(mat).getElementFilter().elementTypes(dim, _not_ghost)) {
    add_array(w, "cauchy/stress/" + std::to_string(type), mat.getStress(type),
              "stress internal after Material::onDump (Cauchy stress, finite deformation)");
  }
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

fs::path write_tmp(const std::string & name, const std::string & text) {
  auto p = fs::temp_directory_path() / name;
  std::ofstream f(p, std::ios::binary | std::ios::trunc);
  f << text;
  return p;
}

/// Plane-strain / 3-D / 1-D finite-deformation steel (the upstream
/// material_check_stress_plane_strain_finite_deformation.dat with
/// finite_deformation = true, UD-210).
std::string fd_material(Int dim, const std::string & extra = "") {
  return std::string("material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n"
                     "  nu = 0.3\n") +
         (dim == 2 ? "  Plane_Stress = false\n" : "") + "  finite_deformation = true\n" + extra +
         "]\n";
}

const fs::path & patch_data() {
  static fs::path p = g_up / "test/test_model/patch_tests/data";
  return p;
}

/// Mesh, material, parse, model; the material is parsed before the model
/// is created (as the upstream fixtures).
struct Setup {
  fs::path material;
  std::string material_source;
};

Setup material_setup(const std::string & tmp_name, const std::string & text) {
  auto p = write_tmp(tmp_name, text);
  getStaticParser().parse(p.string());
  return {p, "generated:smm_fd_probe.cc"};
}

void common_inputs(ap::FixtureWriter & w, const fs::path & mesh, const std::string & mesh_source,
                   const Setup & s) {
  w.add_tag("model");
  w.add_tag("smm");
  w.add_tag("finite_deformation");
  w.add_input_file("mesh", mesh, mesh_source);
  w.add_input_file("material", s.material, s.material_source);
}

void newton(NonLinearSolver & solver, Int max, Real threshold, SolveConvergenceCriteria c) {
  solver.set("max_iterations", max);
  solver.set("threshold", threshold);
  solver.set("convergence_type", c);
}

void newton_params(ap::FixtureWriter & w, Int max, Real threshold, const std::string & c) {
  auto & p = w.parameters();
  p["newton_max_iterations"] = static_cast<long long>(max);
  p["newton_threshold"] = threshold;
  p["newton_convergence_type"] = c;
}

/* -------------------------------------------------------------------------- */
/* patch tests                                                                 */
/* -------------------------------------------------------------------------- */
const Matrix<Real> & alpha() {
  static Matrix<Real> a{{0.01, 0.02, 0.03, 0.04},
                        {0.05, 0.06, 0.07, 0.08},
                        {0.09, 0.10, 0.11, 0.12}};
  return a;
}

/// patch_test_linear_fixture.hh setLinearDOF.
template <typename V1, typename V2> void set_linear_dof(V1 && dof, V2 && coord) {
  for (Int i = 0; i < dof.size(); ++i) {
    dof(i) = alpha()(i, 0);
    for (Int j = 0; j < coord.size(); ++j) {
      dof(i) += alpha()(i, j + 1) * coord(j);
    }
  }
}

void patch_case(ElementType type, AnalysisMethod method, const std::string & analysis,
                bool plane_stress = false) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  std::string id = analysis + "_patch_" + tname.substr(1);
  if (plane_stress) {
    id += "_plane_stress";
  }
  if (not wanted(id)) {
    return;
  }
  const auto mesh_file = patch_data() / (tname + ".msh");
  Setup s;
  if (plane_stress) {
    s.material = patch_data() / "material_check_stress_plane_stress_finite_deformation.dat";
    s.material_source =
        "upstream:test/test_model/patch_tests/data/"
        "material_check_stress_plane_stress_finite_deformation.dat";
    getStaticParser().parse(s.material.string());
  } else {
    s = material_setup("smm_fd_probe_patch.dat", fd_material(dim));
  }
  Mesh mesh(dim);
  mesh.read(mesh_file.string());
  MeshUtils::buildFacets(mesh);
  mesh.createBoundaryGroupFromGeometry();
  SolidMechanicsModel model(mesh, _all_dimensions, tname);
  model.initFull(_analysis_method = method, _solver_options = eigen_options());
  auto & blocked = model.getBlockedDOFs();
  auto & disp = model.getDisplacement();
  const auto & X = mesh.getNodes();
  for (auto & eg : mesh.iterateElementGroups()) {
    for (const auto & node : eg.getNodeGroup()) {
      for (Int c = 0; c < blocked.getNbComponent(); ++c) {
        blocked(node, c) = true;
      }
    }
  }
  for (auto & eg : mesh.iterateElementGroups()) {
    for (const auto & node : eg.getNodeGroup()) {
      set_linear_dof(disp.begin(dim)[node], X.begin(dim)[node]);
    }
  }
  auto & solver = model.getNonLinearSolver();
  const bool is_newton = aka::is_of_type<NonLinearSolverNewtonRaphson>(solver);
  Int max_it = 0;
  Real threshold = 0.;
  if (is_newton) {
    max_it = method == _static ? 10 : 3;
    threshold = method == _static ? 1e-10 : 1e-6;
    newton(solver, max_it, threshold, SolveConvergenceCriteria::_solution);
  }
  Real stable_dt = 0., dt = 0.;
  if (method != _static) {
    stable_dt = model.getStableTimeStep();
    dt = 0.8 * stable_dt;
    model.setTimeStep(dt);
  }
  const std::string cid = kArea + id;
  ap::FixtureWriter w(g_root, cid,
                      "finite-deformation patch test (" + analysis + ", " + tname +
                          (plane_stress ? ", plane stress" : "") +
                          ") on the upstream patch-test mesh, Eigen sparse solver",
                      kProbe, kProbeVersion);
  common_inputs(w, mesh_file, "upstream:test/test_model/patch_tests/data/" + tname + ".msh", s);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  p["model_id"] = tname;
  p["analysis"] = analysis;
  p["newton"] = is_newton;
  if (is_newton) {
    newton_params(w, max_it, threshold, "solution");
  }
  add_bools(w, "blocked_dofs", blocked, "blocked flags after applyBC");
  add_array(w, "initial_displacement", disp, "displacement after applyBC (affine on boundary)");
  std::vector<std::string> digests;
  std::vector<I64> iterations;
  std::vector<double> errors;
  if (method == _static) {
    model.solveStep();
    iterations.push_back(Int(solver.get("nb_iterations")));
    errors.push_back(Real(solver.get("error")));
  } else {
    add_vec(w, "time_step", {stable_dt, dt}, "getStableTimeStep(), 0.8 * getStableTimeStep()");
    for (auto && [Xn, u] : zip(make_view(X, dim), make_view(disp, dim))) {
      set_linear_dof(u, Xn);
    }
    for (Int step = 0; step < 100; ++step) {
      model.solveStep();
      digests.push_back(digest(
          {&model.getDisplacement(), &model.getVelocity(), &model.getAcceleration()}));
      if (is_newton) {
        iterations.push_back(Int(solver.get("nb_iterations")));
        errors.push_back(Real(solver.get("error")));
      }
    }
    add_vec(w, "energy/kinetic_final", {model.getEnergy("kinetic")},
            "getEnergy(\"kinetic\") after the last step");
    add_array(w, "velocity", model.getVelocity(), "final velocity");
    add_array(w, "acceleration", model.getAcceleration(), "final acceleration");
    Json dj = Json::array();
    for (auto & d : digests) {
      dj.push(d);
    }
    p["step_digests"] = dj;
  }
  if (is_newton) {
    add_ivec(w, "newton/nb_iterations", iterations, "nb_iterations per solveStep");
    add_vec(w, "newton/error", errors, "error per solveStep");
  }
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  add_vec(w, "energy/potential", {model.getEnergy("potential")},
          "getEnergy(\"potential\") at the final state");
  record_internals(w, model);
  record_aij(w, "K", model, "K", method == _static);
  if (model.getDOFManager().hasLumpedMatrix("M")) {
    add_array(w, "mass", model.getMass(), "lumped mass (model array)");
  }
  record_cauchy(w, model);
  record_mesh(w, mesh);
  w.finish();
  fs::remove(fs::temp_directory_path() / "smm_fd_probe_patch.dat");
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* x = xmin blocked: dynamic and loaded cases                                  */
/* -------------------------------------------------------------------------- */
Real xmin_of(const Mesh & mesh) {
  const auto & X = mesh.getNodes();
  Real xmin = X(0, 0);
  for (Int n = 0; n < X.size(); ++n) {
    xmin = std::min(xmin, X(n, 0));
  }
  return xmin;
}

Real xmax_of(const Mesh & mesh) {
  const auto & X = mesh.getNodes();
  Real xmax = X(0, 0);
  for (Int n = 0; n < X.size(); ++n) {
    xmax = std::max(xmax, X(n, 0));
  }
  return xmax;
}

void block_xmin(SolidMechanicsModel & model) {
  const auto & X = model.getMesh().getNodes();
  const Real xmin = xmin_of(model.getMesh());
  auto & b = model.getBlockedDOFs();
  for (Int n = 0; n < X.size(); ++n) {
    if (std::abs(X(n, 0) - xmin) < 1e-10) {
      for (Int c = 0; c < b.getNbComponent(); ++c) {
        b(n, c) = true;
      }
    }
  }
}

/// Non-affine initial displacement vanishing on x = xmin:
/// u_i = amplitude (X_0 - xmin) (c_i0 + c_i1 X_1 + c_i2 X_2) (sequential sums).
void initial_field(SolidMechanicsModel & model, Real amplitude) {
  static const Real c[3][3] = {{0.5, 0.3, -0.2}, {-0.4, 0.6, 0.1}, {0.2, -0.1, 0.4}};
  const auto dim = model.getSpatialDimension();
  const auto & X = model.getMesh().getNodes();
  const Real xmin = xmin_of(model.getMesh());
  auto & u = model.getDisplacement();
  for (Int n = 0; n < X.size(); ++n) {
    for (Int i = 0; i < dim; ++i) {
      Real s = c[i][0];
      for (Int j = 1; j < dim; ++j) {
        s += c[i][j] * X(n, j);
      }
      u(n, i) = amplitude * (X(n, 0) - xmin) * s;
    }
  }
}

void dynamic_case(ElementType type, AnalysisMethod method, const std::string & analysis,
                  Int steps) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  const std::string id = "dynamic_" + analysis + "_" + tname.substr(1);
  if (not wanted(id)) {
    return;
  }
  const auto mesh_file = patch_data() / (tname + ".msh");
  auto s = material_setup("smm_fd_probe_dyn.dat", fd_material(dim));
  Mesh mesh(dim);
  mesh.read(mesh_file.string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = method, _solver_options = eigen_options());
  block_xmin(model);
  const Real amplitude = method == _implicit_dynamic ? 0.05 : 0.1;
  initial_field(model, amplitude);
  auto & solver = model.getNonLinearSolver();
  const bool is_newton = aka::is_of_type<NonLinearSolverNewtonRaphson>(solver);
  if (is_newton) {
    newton(solver, 30, 1e-9, SolveConvergenceCriteria::_residual);
  }
  const Real stable = model.getStableTimeStep();
  const Real dt = method == _implicit_dynamic ? 1e-4 : 0.5 * stable;
  model.setTimeStep(dt);
  const std::string cid = kArea + id;
  ap::FixtureWriter w(g_root, cid,
                      "finite-deformation " + analysis + " dynamics on " + tname +
                          ": x = xmin blocked, non-affine initial displacement, " +
                          std::to_string(steps) + " steps (Eigen sparse solver)",
                      kProbe, kProbeVersion);
  common_inputs(w, mesh_file, "upstream:test/test_model/patch_tests/data/" + tname + ".msh", s);
  fs::remove(s.material);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  p["analysis"] = analysis;
  p["steps"] = static_cast<long long>(steps);
  p["amplitude"] = amplitude;
  p["newton"] = is_newton;
  if (is_newton) {
    newton_params(w, 30, 1e-9, "residual");
  }
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep() at the initial state, time step used");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "blocked flags");
  add_array(w, "initial_displacement", model.getDisplacement(), "initial displacement");
  std::vector<double> ek, ep, ew;
  std::vector<I64> its;
  std::vector<double> errs;
  Json dj = Json::array();
  auto snap = [&]() {
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
    ew.push_back(model.getEnergy("external work"));
  };
  snap();
  for (Int st = 0; st < steps; ++st) {
    model.solveStep();
    if (is_newton) {
      its.push_back(Int(solver.get("nb_iterations")));
      errs.push_back(Real(solver.get("error")));
    }
    dj.push(digest({&model.getDisplacement(), &model.getVelocity(), &model.getAcceleration()}));
    snap();
  }
  p["step_digests"] = dj;
  add_vec(w, "energy/kinetic", ek, "getEnergy(\"kinetic\") per state (index 0: initial)");
  add_vec(w, "energy/potential", ep, "getEnergy(\"potential\") per state");
  add_vec(w, "energy/external_work", ew, "getEnergy(\"external work\") per state");
  if (is_newton) {
    add_ivec(w, "newton/nb_iterations", its, "nb_iterations per step");
    add_vec(w, "newton/error", errs, "error per step");
  }
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "velocity", model.getVelocity(), "final velocity");
  add_array(w, "acceleration", model.getAcceleration(), "final acceleration");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  record_internals(w, model);
  record_aij(w, "K", model, "K");
  record_aij(w, "M", model, "M", false);
  if (model.getDOFManager().hasLumpedMatrix("M")) {
    add_array(w, "mass", model.getMass(), "lumped mass (model array)");
  }
  record_cauchy(w, model);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

void loaded_case(ElementType type, bool eigen = false) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  const std::string id =
      (eigen ? std::string("eigenstrain_stiffness_") : std::string("loaded_static_")) +
      tname.substr(1);
  if (not wanted(id)) {
    return;
  }
  const auto mesh_file = patch_data() / (tname + ".msh");
  auto s = material_setup("smm_fd_probe_loaded.dat",
                          fd_material(dim, eigen ? "  eigen_grad_u = [[0.01, 0.002], [0.003, -0.004]]\n"
                                            : ""));
  Mesh mesh(dim);
  mesh.read(mesh_file.string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  const auto & X = mesh.getNodes();
  const Real xmin = xmin_of(mesh), xmax = xmax_of(mesh);
  const Real load[3] = {2.0e9, -1.0e9, 5.0e8};
  auto & blocked = model.getBlockedDOFs();
  auto & fext = model.getExternalForce();
  for (Int n = 0; n < X.size(); ++n) {
    for (Int c = 0; c < dim; ++c) {
      if (std::abs(X(n, 0) - xmin) < 1e-10) {
        blocked(n, c) = true;
      }
      if (std::abs(X(n, 0) - xmax) < 1e-10) {
        fext(n, c) = load[c];
      }
    }
  }
  auto & solver = model.getNonLinearSolver();
  newton(solver, 30, 1e-9, SolveConvergenceCriteria::_residual);
  model.solveStep();
  const std::string cid = kArea + id;
  ap::FixtureWriter w(g_root, cid,
                      std::string(eigen ? "finite-deformation static solve with eigen_grad_u "
                                          "then assembleStiffnessMatrix(true)"
                                        : "finite-deformation large-load static solve") +
                          " on " + tname + ": x = xmin blocked, nodal load on x = xmax "
                                           "(Eigen sparse solver)",
                      kProbe, kProbeVersion);
  common_inputs(w, mesh_file, "upstream:test/test_model/patch_tests/data/" + tname + ".msh", s);
  fs::remove(s.material);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  newton_params(w, 30, 1e-9, "residual");
  add_vec(w, "nodal_load", std::vector<double>(load, load + dim), "load per x = xmax node");
  add_bools(w, "blocked_dofs", blocked, "x = xmin face blocked");
  add_array(w, "external_force", fext, "nodal load on the x = xmax face");
  add_ivec(w, "newton/nb_iterations", {Int(solver.get("nb_iterations"))}, "nb_iterations");
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "error");
  add_array(w, "displacement", model.getDisplacement(), "solution");
  add_array(w, "internal_force", model.getInternalForce(), "internal force");
  add_vec(w, "energy/potential", {model.getEnergy("potential")}, "getEnergy(\"potential\")");
  record_internals(w, model);
  record_aij(w, "K", model, "K");
  if (eigen) {
    model.assembleStiffnessMatrix(true);
    record_internals(w, model, "after_stiffness/");
    record_aij(w, "after_stiffness/K", model, "K");
  }
  record_cauchy(w, model);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* Two materials on a mixed quadrangle_8 / triangle_6 mesh                    */
/* -------------------------------------------------------------------------- */
constexpr const char * kMixedMesh = R"($MeshFormat
2.2 0 8
$EndMeshFormat
$PhysicalNames
6
2 1 "a"
2 2 "b"
1 3 "left"
1 4 "right"
1 5 "bottom"
1 6 "top"
$EndPhysicalNames
$Nodes
14
1 0 0 0
2 1 0 0
3 2 0 0
4 0 1 0
5 1 1 0
6 2 1.1 0
7 0.5 0 0
8 1.5 0 0
9 0.5 1 0
10 1.5 1.05 0
11 0 0.5 0
12 1.05 0.5 0
13 2 0.55 0
14 1.5 0.55 0
$EndNodes
$Elements
9
1 16 2 1 1 1 2 5 4 7 12 9 11
2 9 2 2 2 2 3 6 8 13 14
3 9 2 2 2 2 6 5 14 10 12
4 8 2 3 3 1 4 11
5 8 2 4 4 3 6 13
6 8 2 5 5 1 2 7
7 8 2 5 5 2 3 8
8 8 2 6 6 4 5 9
9 8 2 6 6 5 6 10
$EndElements
)";

std::string mixed_material(bool b_finite) {
  return std::string("material elastic [\n  name = a\n  rho = 1000\n  E = 1e9\n  nu = 0.3\n"
                     "  Plane_Stress = false\n  finite_deformation = true\n]\n"
                     "material elastic [\n  name = b\n  rho = 2000\n  E = 3e9\n  nu = 0.2\n"
                     "  Plane_Stress = false\n  finite_deformation = ") +
         (b_finite ? "true" : "false") + "\n]\n";
}

/// Internals of every material per type of its filter (empty types
/// included): stress, grad_u, piola_kirchhoff_2 (finite deformation only),
/// filter; the type lists in `materials/m<i>/types`.
void record_internals_all(ap::FixtureWriter & w, SolidMechanicsModel & model,
                          const std::string & prefix) {
  auto dim = model.getSpatialDimension();
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    auto & mat = model.getMaterial(m);
    const auto mp = prefix + "m" + std::to_string(m) + "/";
    Json types = Json::array();
    for (auto type :
         static_cast<const Material &>(mat).getElementFilter().elementTypes(dim, _not_ghost)) {
      auto name = std::to_string(type);
      types.push(name);
      add_array(w, mp + "stress/" + name, mat.getStress(type),
                "material stress per quadrature point, raw column-major dim x dim tuples");
      add_array(w, mp + "grad_u/" + name, mat.getGradU(type),
                "material grad_u per quadrature point, raw column-major dim x dim tuples");
      if (mat.isFiniteDeformation()) {
        add_array(w, mp + "piola_kirchhoff_2/" + name,
                  mat.getArray<Real>("piola_kirchhoff_2", type),
                  "second Piola-Kirchhoff stress per quadrature point, raw column-major tuples");
      }
      const auto & cf = static_cast<const Material &>(mat).getElementFilter(type);
      std::vector<I64> f(cf.data(), cf.data() + cf.size());
      add_ivec(w, mp + "filter/" + name, f, "element filter of the material");
    }
    w.parameters()[mp + "types"] = types;
    w.parameters()[mp + "finite_deformation"] = mat.isFiniteDeformation();
  }
}

void mixed_case(bool b_finite, bool linear) {
  const std::string id = std::string("mixed_quad8_tri6_") + (b_finite ? "fd_fd" : "fd_lin") +
                         (linear ? "_linear" : "");
  if (not wanted(id)) {
    return;
  }
  auto mesh_file = write_tmp("smm_fd_probe_mixed.msh", kMixedMesh);
  auto s = material_setup("smm_fd_probe_mixed.dat", mixed_material(b_finite));
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  SolidMechanicsModel model(mesh);
  auto sel = std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  model.setMaterialSelector(sel);
  auto so = eigen_options();
  if (linear) {
    so.non_linear_solver_type = NonLinearSolverType::_linear;
  }
  model.initFull(_analysis_method = _static, _solver_options = so);
  auto & solver = model.getNonLinearSolver();
  if (not linear) {
    newton(solver, 50, 1e-10, SolveConvergenceCriteria::_solution);
  }
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "bottom");
  const std::string cid = kArea + id;
  ap::FixtureWriter w(g_root, cid,
                      std::string("review V47 Appendix A (FD-01): quadrangle_8 material a "
                                  "(finite deformation) + triangle_6 material b (") +
                          (b_finite ? "finite deformation" : "small strain") +
                          ") by physical name, FromSameDim / FromHigherDim loads in two steps, " +
                          (linear ? "_linear non-linear solver" : "static Newton-Raphson") +
                          " (Eigen sparse solver)",
                      kProbe, kProbeVersion);
  common_inputs(w, mesh_file, "generated:smm_fd_probe.cc (review V47 Appendix A mixed.msh)", s);
  fs::remove(s.material);
  fs::remove(mesh_file);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["material_selector"] = "MeshDataMaterialSelector<std::string>(physical_names)";
  p["non_linear_solver"] = linear ? "linear" : "newton_raphson";
  if (not linear) {
    newton_params(w, 50, 1e-10, "solution");
  }
  const std::vector<double> lambdas{0.5, 1.0};
  add_vec(w, "load_factors", lambdas, "lambda of the two load steps");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "left blocked, bottom y blocked");
  for (std::size_t k = 0; k < lambdas.size(); ++k) {
    const Real l = lambdas[k];
    model.getExternalForce().zero();
    Vector<Real> traction{l * 1e8, l * 2e8};
    model.applyBC(BC::Neumann::FromSameDim(traction), "right");
    Matrix<Real> sigma{{0., l * 3e7}, {l * 3e7, l * -2e7}};
    model.applyBC(BC::Neumann::FromHigherDim(sigma), "top");
    model.solveStep();
    const auto sp = "step" + std::to_string(k + 1) + "/";
    if (not linear) {
      add_ivec(w, sp + "newton/nb_iterations", {Int(solver.get("nb_iterations"))},
               "nb_iterations");
      add_vec(w, sp + "newton/error", {Real(solver.get("error"))}, "error");
    }
    add_array(w, sp + "external_force", model.getExternalForce(), "f_ext of the step");
    add_array(w, sp + "displacement", model.getDisplacement(), "solution");
    add_array(w, sp + "internal_force", model.getInternalForce(), "internal force");
    record_aij(w, sp + "K", model, "K");
    record_internals_all(w, model, sp);
  }
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    auto & mat = model.getMaterial(m);
    mat.onDump();
    for (auto type :
         static_cast<const Material &>(mat).getElementFilter().elementTypes(2, _not_ghost)) {
      add_array(w, "cauchy/m" + std::to_string(m) + "/stress/" + std::to_string(type),
                mat.getStress(type),
                "stress internal after Material::onDump (Cauchy stress for finite deformation)");
    }
  }
  add_vec(w, "energy/potential", {model.getEnergy("potential")}, "getEnergy(\"potential\")");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* upstream tests                                                              */
/* -------------------------------------------------------------------------- */
/// test_solid_mechanics_model_material_large_rotation.cc (without the
/// dumps and prints).
void large_rotation_case() {
  const std::string id = "large_rotation_tetrahedron_4";
  if (not wanted(id)) {
    return;
  }
  const auto dir = g_up / "test/test_model/test_solid_mechanics_model";
  Setup s{dir / "material_elastic.dat",
          "upstream:test/test_model/test_solid_mechanics_model/material_elastic.dat"};
  getStaticParser().parse(s.material.string());
  const Int dim = 3;
  Mesh mesh(dim);
  mesh.read((dir / "cube_3d_tet_4.msh").string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _implicit_dynamic, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver();
  solver.set("threshold", 1e-4);
  solver.set("max_iterations", 100);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  model.assembleMass();
  // The upstream test reads M(i, j) through the non-const
  // SparseMatrixAIJ::operator(), which asserts that (i, j) is in the profile
  // (UD-078: the oracle aborts on "Couple (i,j) = (0,3) does not exist in
  // the profile"; without assertions it dereferences end()). The const
  // accessor returns 0 outside the profile, which is the test's intent.
  const auto & M = aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix("M"));
  Array<Real> mass(M.size(), 1);
  mass.zero();
  for (Int i = 0; i < M.size(); ++i) {
    for (Int j = 0; j < M.size(); ++j) {
      mass[i] += M(i, j);
    }
  }
  std::array<Real, 3> mass_center{0., 0., 0.};
  Real total_mass = 0.;
  // upstream loops i < mass.size() and reads mass(i * 3 + j): out of range
  // for i >= size / 3 (second test defect, UD-078); the in-range part:
  for (Int i = 0; i < mass.size() / 3; ++i) {
    for (Int j = 0; j < 3; ++j) {
      mass_center[j] += mass(i * 3 + j);
      total_mass += mass(i * 3 + j);
    }
  }
  mass_center[0] /= total_mass / 3.;
  mass_center[1] /= total_mass / 3.;
  mass_center[2] /= total_mass / 3.;
  const Real dt = model.getTimeStep();
  model.solveStep();
  const std::string cid = kArea + id;
  ap::FixtureWriter w(g_root, cid,
                      "test_solid_mechanics_model_material_large_rotation.cc (UNSTABLE upstream): "
                      "implicit dynamic finite-deformation model, mass centre, one solveStep",
                      kProbe, kProbeVersion);
  common_inputs(w, dir / "cube_3d_tet_4.msh",
                "upstream:test/test_model/test_solid_mechanics_model/cube_3d_tet_4.msh", s);
  auto & p = w.parameters();
  p["spatial_dimension"] = 3LL;
  p["element_type"] = "_tetrahedron_4";
  add_vec(w, "time_step", {dt}, "getTimeStep() (default) at solveStep");
  add_array(w, "mass_row_sums", mass, "sum_j M(i, j) (upstream loop, M(i, j) lookups)");
  add_vec(w, "mass_center", {mass_center[0], mass_center[1], mass_center[2]},
          "mass centre as the upstream test computes it");
  add_vec(w, "total_mass", {total_mass}, "total mass (sum over all DOFs)");
  add_ivec(w, "newton/nb_iterations", {Int(solver.get("nb_iterations"))}, "nb_iterations");
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "error");
  add_array(w, "displacement", model.getDisplacement(), "displacement after solveStep");
  add_array(w, "internal_force", model.getInternalForce(), "internal force after solveStep");
  record_aij(w, "M", model, "M");
  record_aij(w, "K", model, "K", false);
  record_internals(w, model);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/// test_finite_deformation.cc (TestFiniteDeformation.NotUnit).
void rotation_case() {
  const std::string id = "rotation_tetrahedron_4";
  if (not wanted(id)) {
    return;
  }
  const auto dir = g_up / "test/test_model/test_solid_mechanics_model/test_materials";
  Setup s{dir / "material_finite_deformation.dat",
          "upstream:test/test_model/test_solid_mechanics_model/test_materials/"
          "material_finite_deformation.dat"};
  getStaticParser().parse(s.material.string());
  const double pi = std::atan(1) * 4;
  constexpr int dim = 3;
  Mesh mesh(dim);
  mesh.read((dir / "1_tetrahedron.msh").string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  Matrix<Real> alpha{{0.00, 0.02, 0.03, 0.04},
                     {0.00, 0.06, 0.07, 0.08},
                     {0.00, 0.10, 0.11, 0.12}};
  auto impose_disp = [&] {
    model.getDisplacement().zero();
    for (auto data : zip(make_view(mesh.getNodes(), dim), make_view(model.getDisplacement(), dim),
                         make_view(model.getBlockedDOFs(), dim))) {
      auto & pos = std::get<0>(data);
      auto & dis = std::get<1>(data);
      auto & blocked = std::get<2>(data);
      blocked.set(true);
      dis += alpha(0);
      for (auto p : arange(dim)) {
        dis += alpha(1 + p) * pos(p);
      }
    }
  };
  auto & solver = model.getNonLinearSolver();
  impose_disp();
  model.solveStep();
  const std::vector<I64> it0{Int(solver.get("nb_iterations"))};
  const std::vector<double> err0{Real(solver.get("error"))};
  auto displacement0 = model.getDisplacement();
  auto internal_force0 = model.getInternalForce();
  auto theta = pi / 4;
  Matrix<Real> R{{1., 0., 0.},
                 {0., std::cos(theta), -std::sin(theta)},
                 {0., std::sin(theta), std::cos(theta)}};
  impose_disp();
  for (auto data : zip(make_view(mesh.getNodes(), dim), make_view(model.getDisplacement(), dim))) {
    auto & X = std::get<0>(data);
    auto & u = std::get<1>(data);
    u = R * (X + u) - X;
  }
  Array<Real> rotated = model.getDisplacement();
  model.solveStep();
  const std::string cid = kArea + id;
  ap::FixtureWriter w(g_root, cid,
                      "test_finite_deformation.cc (TestFiniteDeformation.NotUnit): affine "
                      "field, then rotated by pi/4 about x, static solves",
                      kProbe, kProbeVersion);
  common_inputs(w, dir / "1_tetrahedron.msh",
                "upstream:test/test_model/test_solid_mechanics_model/test_materials/"
                "1_tetrahedron.msh",
                s);
  auto & p = w.parameters();
  p["spatial_dimension"] = 3LL;
  p["element_type"] = "_tetrahedron_4";
  add_vec(w, "rotation", {R(0, 0), R(1, 0), R(2, 0), R(0, 1), R(1, 1), R(2, 1), R(0, 2), R(1, 2),
                          R(2, 2)},
          "R column-major");
  add_ivec(w, "first/newton/nb_iterations", it0, "nb_iterations of the first solve");
  add_vec(w, "first/newton/error", err0, "error of the first solve");
  add_array(w, "first/displacement", displacement0, "displacement after the first solve");
  add_array(w, "first/internal_force", internal_force0, "internal force after the first solve");
  add_array(w, "rotated_displacement", rotated, "R (X + u) - X imposed before the second solve");
  add_ivec(w, "newton/nb_iterations", {Int(solver.get("nb_iterations"))}, "nb_iterations");
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "error");
  add_array(w, "displacement", model.getDisplacement(), "displacement after the second solve");
  add_array(w, "internal_force", model.getInternalForce(), "internal force after the second solve");
  record_internals(w, model);
  record_aij(w, "K", model, "K");
  record_cauchy(w, model);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: smm_fd_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>\n";
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
    for (auto type : all) {
      patch_case(type, _static, "static");
      if (Mesh::getSpatialDimension(type) == 2) {
        patch_case(type, _static, "static", true);
      }
      patch_case(type, _explicit_lumped_mass, "explicit");
      patch_case(type, _implicit_dynamic, "implicit");
      dynamic_case(type, _explicit_lumped_mass, "explicit", 50);
      dynamic_case(type, _implicit_dynamic, "implicit", 10);
      loaded_case(type);
    }
    loaded_case(_quadrangle_4, true);
    large_rotation_case();
    rotation_case();
    mixed_case(true, false);
    mixed_case(true, true);
    mixed_case(false, false);
  } catch (std::exception & e) {
    std::cerr << "smm_fd_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
