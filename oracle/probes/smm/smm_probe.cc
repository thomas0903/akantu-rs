// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs SolidMechanicsModel scenarios with the Eigen sparse
// solver (so that every solve is reproducible by the port's bit-identical
// SparseLU translation, D-008) and records the model state for the Rust
// differential tests (fixtures/model/smm/*).
//
// Cases (fixture area model, case ids model/smm/<case>, format v1):
//   patch_<analysis>_<law>_<type>[_<plane>]
//       the upstream patch tests (test/test_model/patch_tests/
//       patch_test_linear_{elastic,anisotropic}_{explicit,implicit}.cc,
//       patch_test_linear_fixture.hh, ..._solid_mechanics_fixture.hh) for
//       every patch-test mesh: buildFacets + createBoundaryGroupFromGeometry,
//       nodes of every element group blocked at the affine field, explicit /
//       implicit dynamic (100 steps, dt = 0.8 getStableTimeStep) or static
//       (Newton-Raphson, max_iterations 3, threshold 1e-6, solution
//       criterion); then the checkForces Neumann load FromHigherDim(sigma) on
//       every element group. Laws: elastic (plane strain / plane stress) and
//       elastic_anisotropic (material_anisotropic_<dim>.dat); finite
//       deformation for 4 types (explicit / implicit). The implicit-dynamic
//       anisotropic cases (patch_implicit_anisotropic_*) are probe-defined:
//       upstream has AnisotropicExplicit and AnisotropicStatic only
//       (the static ones are patch_static_anisotropic_*).
//   lumped_mass_<type>      test_lumped_mass.cc (material_lumped.dat)
//   eigenstrain_tetrahedron_4
//                           test_solid_mechanics_model_material_eigenstrain.cc
//   reassign_cube_two_materials
//                           test_solid_mechanics_model_reassign_material.cc
//                           (frozen mesh), plus a static solve afterwards
//   consistent_explicit_<type>, implicit_dynamic_<type>
//                           probe scenarios: linear initial velocity field,
//                           x = xmin face blocked, 20 / 10 steps
//   bc_functors_<type>      Dirichlet FixedValue / IncrementValue / Increment
//                           / FlagOnly and Neumann FromSameDim / FromHigherDim
//                           / FreeBoundary on the boundary group, recorded
//                           after each application
//   patch_static_anisotropic_<type>
//                           TestPatchTestSMMLinear.AnisotropicStatic of
//                           patch_test_linear_anisotropic_implicit.cc (static,
//                           max_iterations 2, threshold 1e-10, residual
//                           criterion)
//   review_mixed_static, review_consistent_blocked_change
//                           review adversarial cases (checkerboard
//                           steel/alu hex8 mesh with boundary quad4 groups in
//                           the main mesh, physical-name selector): the
//                           material map holds only the volume elements
//   eigenstrain_stiffness_tetrahedron_4
//                           eigenstrain_tetrahedron_4 followed by
//                           assembleStiffnessMatrix(true): grad_u left by the
//                           stiffness assembly (upstream: without
//                           -eigen_grad_u, UD-208)
//   loaded_static_<type>    the Neumann-loaded static cases of
//                           fixtures/model/static/loaded_* (model_probe.py
//                           run_loaded) with the Eigen sparse solver
//   explicit_lumped_kinetic_<type>
//                           (probe v2) explicit lumped
//                           mass: which of K, M, C, J / lumped M exist after
//                           initFull, getEnergy("kinetic") before the first
//                           step (consistent-mass branch: the scheme created
//                           a sparse M at setIntegrationScheme), after one
//                           step (lumped branch)
//   periodic_static_<dim>d_<n>
//                           (probe v2) TestPeriodicStatic
//                           (test_periodic_static_fixture.hh, tensile load)
//                           on the frozen line/square/cube.msh made periodic
//                           in the first n directions: state after every
//                           Dirichlet / Neumann functor, then the static
//                           solve with the Eigen solver; 1-D: the nodal load
//                           of the upstream test, and the refusal of a
//                           Neumann functor (AKANTU_TO_IMPLEMENT) recorded
//
// Usage: smm_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: SMM_PROBE_CASES=<id>[,<id>...] restricts the run to the
// listed case ids (without the model/smm/ prefix; unset: every case).

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

#include <Eigen/Core>

#include <cmath>
#include <cstdlib>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "smm_probe";
// v2: cases explicit_lumped_kinetic_* and periodic_static_*; D-050 Eigen
// cache sizes pinned in main (the v1 cases are unchanged).
constexpr const char * kProbeVersion = "2";

using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up, g_frozen;

/// SMM_PROBE_CASES filter (unset: every case); `cid` = model/smm/<id>.
bool wanted(const std::string & cid) {
  const char * env = std::getenv("SMM_PROBE_CASES");
  if (env == nullptr || *env == '\0') {
    return true;
  }
  const std::string prefix = "model/smm/";
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

/// Mesh after the probe's topology changes: nodes, every connectivity (all
/// dimensions), every element group (dimension, elements per type, nodes).
void record_mesh(ap::FixtureWriter & w, const Mesh & mesh) {
  add_array(w, "mesh/nodes", mesh.getNodes(), "node coordinates");
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

/// Raw internal buffers (column-major dim x dim tuples) of material `m`.
void record_internals(ap::FixtureWriter & w, SolidMechanicsModel & model, Int m,
                      const std::string & prefix = "", bool full = true) {
  auto dim = model.getSpatialDimension();
  auto & mat = model.getMaterial(m);
  for (auto type : static_cast<const Material &>(mat).getElementFilter().elementTypes(dim, _not_ghost)) {
    auto name = std::to_string(type);
    if (not full) {
      w.parameters()[prefix + "stress/" + name + "/sha256"] = digest({&mat.getStress(type)});
      w.parameters()[prefix + "grad_u/" + name + "/sha256"] = digest({&mat.getGradU(type)});
      continue;
    }
    add_array(w, prefix + "stress/" + name, mat.getStress(type),
              "material stress per quadrature point, raw column-major dim x dim tuples");
    add_array(w, prefix + "grad_u/" + name, mat.getGradU(type),
              "material grad_u per quadrature point, raw column-major dim x dim tuples");
    const auto & cf = static_cast<const Material &>(mat).getElementFilter(type);
    std::vector<I64> f(cf.data(), cf.data() + cf.size());
    add_ivec(w, prefix + "filter/" + name, f, "element filter of the material");
  }
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
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

struct PatchSpec {
  ElementType type;
  AnalysisMethod method;
  std::string analysis; // static / explicit / implicit
  std::string law;      // elastic / anisotropic / fd
  std::string material; // file under patch_tests/data
  std::string plane;    // "" / plane_strain / plane_stress
  bool anisotropic_static = false; // AnisotropicStatic solver settings
};

void patch_case(const PatchSpec & s) {
  const auto dim = Mesh::getSpatialDimension(s.type);
  const auto tname = std::to_string(s.type);
  std::string cid = "model/smm/patch_" + s.analysis + "_" + s.law + "_" + tname.substr(1);
  if (not s.plane.empty()) {
    cid += "_" + s.plane;
  }
  if (not wanted(cid)) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  MeshUtils::buildFacets(mesh);
  mesh.createBoundaryGroupFromGeometry();
  SolidMechanicsModel model(mesh, _all_dimensions, tname);
  getStaticParser().parse((data / s.material).string());
  model.initFull(_analysis_method = s.method, _solver_options = eigen_options());
  // applyBC: block every node of every element group at the affine field
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
  const bool newton = aka::is_of_type<NonLinearSolverNewtonRaphson>(solver);
  if (newton and s.anisotropic_static) {
    // patch_test_linear_anisotropic_implicit.cc AnisotropicStatic
    solver.set("max_iterations", 2);
    solver.set("threshold", 1e-10);
    solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  } else if (newton) {
    solver.set("max_iterations", 3);
    solver.set("threshold", 1e-6);
    solver.set("convergence_type", SolveConvergenceCriteria::_solution);
  }
  Real stable_dt = 0., dt = 0.;
  if (s.method != _static) {
    stable_dt = model.getStableTimeStep();
    dt = 0.8 * stable_dt;
    model.setTimeStep(dt);
  }
  ap::FixtureWriter w(g_root, cid,
                      "patch test (" + s.analysis + ", " + s.law + ", " + tname +
                          (s.plane.empty() ? "" : ", " + s.plane) +
                          ") of test/test_model/patch_tests with the Eigen sparse solver",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", data / s.material,
                   "upstream:test/test_model/patch_tests/data/" + s.material);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  p["model_id"] = tname;
  p["analysis"] = s.analysis;
  p["law"] = s.law;
  p["sparse_solver_type"] = "eigen";
  p["newton"] = newton;
  if (s.anisotropic_static) {
    p["newton_max_iterations"] = 2LL;
    p["newton_threshold"] = 1e-10;
    p["newton_convergence_type"] = "residual";
  }
  add_bools(w, "blocked_dofs", blocked, "blocked flags after applyBC");
  add_array(w, "initial_displacement", disp, "displacement after applyBC (affine on boundary)");
  std::vector<std::string> digests;
  std::vector<I64> iterations;
  std::vector<double> errors, ekin;
  if (s.method == _static) {
    model.solveStep();
    iterations.push_back(Int(solver.get("nb_iterations")));
    errors.push_back(Real(solver.get("error")));
  } else {
    p["stable_time_step"] = stable_dt;
    p["time_step"] = dt;
    add_vec(w, "time_step", {stable_dt, dt}, "getStableTimeStep(), 0.8 * getStableTimeStep()");
    for (auto && [Xn, u] : zip(make_view(X, dim), make_view(disp, dim))) {
      set_linear_dof(u, Xn);
    }
    for (Int step = 0; step < 100; ++step) {
      model.solveStep();
      digests.push_back(digest(
          {&model.getDisplacement(), &model.getVelocity(), &model.getAcceleration()}));
      if (newton) {
        iterations.push_back(Int(solver.get("nb_iterations")));
        errors.push_back(Real(solver.get("error")));
      }
    }
    ekin.push_back(model.getEnergy("kinetic"));
    add_vec(w, "energy/kinetic_final", ekin, "getEnergy(\"kinetic\") after the last step");
    add_array(w, "velocity", model.getVelocity(), "final velocity");
    add_array(w, "acceleration", model.getAcceleration(), "final acceleration");
    Json dj = Json::array();
    for (auto & d : digests) {
      dj.push(d);
    }
    p["step_digests"] = dj;
    p["step_digests_definition"] =
        "SHA-256 of displacement || velocity || acceleration (binary64 LE) after each solveStep";
  }
  if (newton) {
    add_ivec(w, "newton/nb_iterations", iterations, "nb_iterations per solveStep");
    add_vec(w, "newton/error", errors, "error per solveStep");
  }
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  record_internals(w, model, 0);
  record_aij(w, "K", model, "K", s.method == _static);
  record_aij(w, "M", model, "M", false);
  if (model.getDOFManager().hasLumpedMatrix("M")) {
    add_array(w, "mass", model.getMass(), "lumped mass (model array)");
  }
  // checkForces: external force from FromHigherDim(sigma of quad 0 of element 0)
  auto & mat = model.getMaterial(0);
  Matrix<Real> sigma = make_view(mat.getStress(s.type), dim, dim).begin()[0];
  auto & fext = model.getExternalForce();
  fext.zero();
  if (dim > 1) {
    for (auto & eg : mesh.iterateElementGroups()) {
      model.applyBC(BC::Neumann::FromHigherDim(sigma), eg.getName());
    }
  } else {
    fext(0) = -sigma(0, 0);
    fext(1) = sigma(0, 0);
  }
  add_array(w, "external_force_neumann", fext,
            "checkForces: external force after applyBC(FromHigherDim(sigma), group) on every "
            "element group (sigma = stress of quadrature point 0)");
  record_mesh(w, mesh);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
void lumped_mass_case(ElementType type) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  if (not wanted("model/smm/lumped_mass_" + tname.substr(1))) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  getStaticParser().parse((data / "material_lumped.dat").string());
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  model.assembleMassLumped();
  ap::FixtureWriter w(g_root, "model/smm/lumped_mass_" + tname.substr(1),
                      "test_lumped_mass.cc on " + tname + " (material_lumped.dat)", kProbe,
                      kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", data / "material_lumped.dat",
                   "upstream:test/test_model/patch_tests/data/material_lumped.dat");
  w.parameters()["spatial_dimension"] = static_cast<long long>(dim);
  w.parameters()["element_type"] = tname;
  add_array(w, "mass", model.getMass(), "assembleMassLumped(): model mass array");
  add_vec(w, "stable_time_step", {model.getStableTimeStep()}, "getStableTimeStep()");
  record_mesh(w, mesh);
  w.finish();
  std::cout << "  model/smm/lumped_mass_" << tname.substr(1) << std::endl;
}

/* -------------------------------------------------------------------------- */
void eigenstrain_case(bool stiffness_after = false) {
  const std::string cid = stiffness_after ? "model/smm/eigenstrain_stiffness_tetrahedron_4"
                                          : "model/smm/eigenstrain_tetrahedron_4";
  if (not wanted(cid)) {
    return;
  }
  const auto dir = g_up / "test/test_model/test_solid_mechanics_model";
  getStaticParser().parse((dir / "material_elastic_plane_strain.dat").string());
  const Int dim = 3;
  Matrix<Real> prescribed_eigengradu(dim, dim);
  prescribed_eigengradu.set(0.1);
  Mesh mesh(dim);
  mesh.read((dir / "cube_3d_tet_4.msh").string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver("static");
  solver.set("threshold", 2e-4);
  solver.set("max_iterations", 2);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  const auto & coordinates = mesh.getNodes();
  auto & displacement = model.getDisplacement();
  auto & boundary = model.getBlockedDOFs();
  MeshUtils::buildFacets(mesh);
  mesh.createBoundaryGroupFromGeometry();
  for (auto & group : mesh.iterateElementGroups()) {
    for (const auto & n : group.getNodeGroup()) {
      for (Int i = 0; i < dim; ++i) {
        displacement(n, i) = alpha()(i, 0);
        for (Int j = 0; j < dim; ++j) {
          displacement(n, i) += alpha()(i, j + 1) * coordinates(n, j);
        }
        boundary(n, i) = true;
      }
    }
  }
  auto & eig = model.getMaterial(0).getInternal<Real>("eigen_grad_u")(_tetrahedron_4);
  for (auto && e : make_view(eig, dim, dim)) {
    e = prescribed_eigengradu;
  }
  model.solveStep();
  if (stiffness_after) {
    // The stiffness assembly overwrites grad_u with grad(u)
    // without subtracting eigen_grad_u (material.cc:262-312, UD-208)
    model.assembleStiffnessMatrix(true);
  }
  ap::FixtureWriter w(g_root, cid,
                      stiffness_after
                          ? "test_solid_mechanics_model_material_eigenstrain.cc (Eigen sparse "
                            "solver), then assembleStiffnessMatrix(true): grad_u of the stiffness "
                            "assembly"
                          : "test_solid_mechanics_model_material_eigenstrain.cc (Eigen sparse solver)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", dir / "cube_3d_tet_4.msh",
                   "upstream:test/test_model/test_solid_mechanics_model/cube_3d_tet_4.msh");
  w.add_input_file("material", dir / "material_elastic_plane_strain.dat",
                   "upstream:test/test_model/test_solid_mechanics_model/"
                   "material_elastic_plane_strain.dat");
  w.parameters()["spatial_dimension"] = 3LL;
  w.parameters()["eigen_grad_u"] = 0.1;
  if (stiffness_after) {
    w.parameters()["stiffness_reassembled_after_solve"] = true;
  }
  add_bools(w, "blocked_dofs", boundary, "blocked flags");
  add_ivec(w, "newton/nb_iterations", {Int(solver.get("nb_iterations"))}, "nb_iterations");
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "error");
  add_array(w, "displacement", model.getDisplacement(), "solution");
  add_array(w, "internal_force", model.getInternalForce(), "internal force");
  record_internals(w, model, 0);
  record_aij(w, "K", model, "K");
  record_mesh(w, mesh);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/// test_solid_mechanics_model_reassign_material.cc selector.
class StraightInterfaceMaterialSelector : public ConstitutiveLawSelector {
public:
  StraightInterfaceMaterialSelector(SolidMechanicsModel & model, Int horizontal,
                                    Real pos_interface)
      : model(model), horizontal(horizontal), pos_interface(pos_interface) {
    auto & mesh = model.getMesh();
    auto dim = mesh.getSpatialDimension();
    barycenters.initialize(mesh, _spatial_dimension = dim, _nb_component = dim,
                           _with_nb_element = true);
    // Upstream's test writes the barycentre into a *copy* (`Vector<Real>
    // bary(barycenters.get(el))`), so its barycentres stay 0 and every
    // element selects mat_2 before and after the move (the test passes
    // vacuously; reported as an upstream test defect). The probe stores
    // the barycentre so that the reassignment really moves elements.
    for_each_element(mesh, [&](auto && el) {
      Vector<Real> bary(dim);
      mesh.getBarycenter(el, bary);
      auto && stored = barycenters.get(el);
      for (Int i = 0; i < dim; ++i) {
        stored(i) = bary(i);
      }
    });
  }
  Int operator()(const Element & elem) override {
    if (not set) {
      ids[0] = model.getMaterialIndex("mat_1");
      ids[1] = model.getMaterialIndex("mat_2");
      set = true;
    }
    const auto bary = barycenters.get(elem);
    return bary(horizontal) < pos_interface ? ids[0] : ids[1];
  }
  void move(Real pos, Int h) {
    pos_interface = pos;
    horizontal = h;
    model.reassignConstitutiveLaw();
  }

private:
  SolidMechanicsModel & model;
  ElementTypeMapArray<Real> barycenters;
  std::array<Int, 2> ids{};
  Int horizontal;
  Real pos_interface;
  bool set{false};
};

void record_material_index(ap::FixtureWriter & w, SolidMechanicsModel & model,
                           const std::string & prefix) {
  const auto & mesh = model.getMesh();
  for (auto type : mesh.elementTypes(mesh.getSpatialDimension(), _not_ghost)) {
    const auto & idx = model.getMaterialByElement(type);
    const auto & loc = model.getMaterialLocalNumbering(type);
    std::vector<I64> a(idx.data(), idx.data() + idx.size());
    std::vector<I64> b(loc.data(), loc.data() + loc.size());
    add_ivec(w, prefix + "material_index/" + std::to_string(type), a, "material per element");
    add_ivec(w, prefix + "local_numbering/" + std::to_string(type), b,
             "law-local index per element");
  }
}

void record_filters(ap::FixtureWriter & w, SolidMechanicsModel & model,
                    const std::string & prefix) {
  const auto & mesh = model.getMesh();
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    const auto & mat = static_cast<const Material &>(model.getMaterial(m));
    for (auto type : mesh.elementTypes(mesh.getSpatialDimension(), _not_ghost)) {
      std::vector<I64> v;
      if (mat.getElementFilter().exists(type, _not_ghost)) {
        const auto & f = mat.getElementFilter(type);
        v.assign(f.data(), f.data() + f.size());
      }
      add_ivec(w, prefix + "filter/mat_" + std::to_string(m + 1) + "/" + std::to_string(type), v,
               "element filter of the material");
    }
  }
}

void reassign_case() {
  if (not wanted("model/smm/reassign_cube_two_materials")) {
    return;
  }
  const auto dir = g_up / "test/test_model/test_solid_mechanics_model";
  const auto mesh_file = g_frozen / "test/test_model/test_solid_mechanics_model/cube_two_materials.msh";
  getStaticParser().parse((dir / "two_materials.dat").string());
  Mesh mesh(3);
  mesh.read(mesh_file.string());
  SolidMechanicsModel model(mesh);
  auto sel = std::make_shared<StraightInterfaceMaterialSelector>(model, _x, 0.);
  model.setMaterialSelector(sel);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  ap::FixtureWriter w(g_root, "model/smm/reassign_cube_two_materials",
                      "test_solid_mechanics_model_reassign_material.cc (frozen "
                      "cube_two_materials.msh, barycentres actually computed)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", mesh_file,
                   "frozen:test/test_model/test_solid_mechanics_model/cube_two_materials.msh");
  w.add_input_file("material", dir / "two_materials.dat",
                   "upstream:test/test_model/test_solid_mechanics_model/two_materials.dat");
  w.parameters()["spatial_dimension"] = 3LL;
  record_material_index(w, model, "before/");
  record_filters(w, model, "before/");
  sel->move(0., _y);
  record_material_index(w, model, "after/");
  record_filters(w, model, "after/");
  // No solve afterwards: upstream ConstitutiveLaw::removeElements builds its
  // old-local -> new-local renumbering with a counter that only advances on
  // kept elements and leaves entries of the (uninitialised) renumbering
  // array unset, so the compacted internals (eigen_grad_u, ...) of a law
  // that lost elements hold garbage: two runs of a solve after a real
  // reassignment differ (observed). The assignment itself is deterministic.
  w.finish();
  std::cout << "  model/smm/reassign_cube_two_materials" << std::endl;
}

/* -------------------------------------------------------------------------- */
/// Linear initial velocity field v_i = sum_j beta(i, j) X_j (probe-defined).
void linear_velocity(SolidMechanicsModel & model) {
  static const Real beta[3][3] = {{1e-2, -3e-3, 2e-3}, {4e-3, 5e-3, -1e-3}, {-2e-3, 1e-3, 6e-3}};
  auto dim = model.getSpatialDimension();
  const auto & X = model.getMesh().getNodes();
  auto & v = model.getVelocity();
  for (Int n = 0; n < X.size(); ++n) {
    for (Int i = 0; i < dim; ++i) {
      Real s = 0.;
      for (Int j = 0; j < dim; ++j) {
        s += beta[i][j] * X(n, j);
      }
      v(n, i) = s;
    }
  }
}

void block_xmin(SolidMechanicsModel & model) {
  const auto & X = model.getMesh().getNodes();
  Real xmin = X(0, 0);
  for (Int n = 0; n < X.size(); ++n) {
    xmin = std::min(xmin, X(n, 0));
  }
  auto & b = model.getBlockedDOFs();
  for (Int n = 0; n < X.size(); ++n) {
    if (std::abs(X(n, 0) - xmin) < 1e-10) {
      for (Int c = 0; c < b.getNbComponent(); ++c) {
        b(n, c) = true;
      }
    }
  }
}

void dynamic_case(ElementType type, AnalysisMethod method, const std::string & prefix,
                  Int steps) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  if (not wanted("model/smm/" + prefix + "_" + tname.substr(1))) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  const std::string dat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n"
                          "  nu = 0.3\n]\n";
  auto tmp = fs::temp_directory_path() / "smm_probe_dyn.dat";
  {
    std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
    f << dat;
  }
  getStaticParser().parse(tmp.string());
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = method, _solver_options = eigen_options());
  block_xmin(model);
  linear_velocity(model);
  auto & solver = model.getNonLinearSolver();
  const bool newton = aka::is_of_type<NonLinearSolverNewtonRaphson>(solver);
  if (newton) {
    solver.set("max_iterations", 20);
    solver.set("threshold", 1e-10);
    solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  }
  const Real stable = model.getStableTimeStep();
  const Real dt = method == _implicit_dynamic ? 1e-4 : 0.5 * stable;
  model.setTimeStep(dt);
  const std::string cid = "model/smm/" + prefix + "_" + tname.substr(1);
  ap::FixtureWriter w(g_root, cid,
                      prefix + " on " + tname + ": x = xmin blocked, linear initial velocity, " +
                          std::to_string(steps) + " steps (Eigen sparse solver)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", tmp, "generated:smm_probe.cc");
  fs::remove(tmp);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  p["steps"] = static_cast<long long>(steps);
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "blocked flags");
  add_array(w, "initial_velocity", model.getVelocity(), "initial velocity");
  std::vector<double> ek, ep, ew, ewn, u_all, v_all, a_all;
  std::vector<I64> its;
  std::vector<double> errs;
  auto snap = [&]() {
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
    ew.push_back(model.getEnergy("external work"));
    ewn.push_back(model.getEnergy("external work new"));
    for (auto x : vec_of(model.getDisplacement())) u_all.push_back(x);
    for (auto x : vec_of(model.getVelocity())) v_all.push_back(x);
    for (auto x : vec_of(model.getAcceleration())) a_all.push_back(x);
  };
  snap();
  for (Int s = 0; s < steps; ++s) {
    model.solveStep();
    if (newton) {
      its.push_back(Int(solver.get("nb_iterations")));
      errs.push_back(Real(solver.get("error")));
    }
    snap();
  }
  const auto nn = static_cast<std::size_t>(mesh.getNbNodes());
  const auto ns = static_cast<std::size_t>(steps + 1);
  for (auto && [name, v] : std::vector<std::pair<std::string, std::vector<double> *>>{
           {"displacement", &u_all}, {"velocity", &v_all}, {"acceleration", &a_all}}) {
    w.add(name, {ns, nn, static_cast<std::size_t>(dim)}, {"step", "node", "component"}, "nodal",
          *v)
        .description("state before the first step (index 0) and after every step");
  }
  add_vec(w, "energy/kinetic", ek, "getEnergy(\"kinetic\") per state");
  add_vec(w, "energy/potential", ep, "getEnergy(\"potential\") per state");
  add_vec(w, "energy/external_work", ew, "getEnergy(\"external work\") per state");
  add_vec(w, "energy/external_work_new", ewn, "getEnergy(\"external work new\") per state");
  if (newton) {
    add_ivec(w, "newton/nb_iterations", its, "nb_iterations per step");
    add_vec(w, "newton/error", errs, "error per step");
  }
  add_vec(w, "energy/kinetic_element_0",
          {model.getEnergy("kinetic", Element{type, 0, _not_ghost})},
          "getEnergy(\"kinetic\", element 0) after the last step");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  record_internals(w, model, 0);
  record_aij(w, "K", model, "K");
  record_aij(w, "M", model, "M");
  if (model.getDOFManager().hasLumpedMatrix("M")) {
    add_array(w, "mass", model.getMass(), "lumped mass (model array)");
  }
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
void bc_case(ElementType type) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  if (not wanted("model/smm/bc_functors_" + tname.substr(1))) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  getStaticParser().parse((data / "material_check_stress_plane_strain.dat").string());
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  MeshUtils::buildFacets(mesh);
  mesh.createBoundaryGroupFromGeometry();
  SolidMechanicsModel model(mesh, _all_dimensions, tname);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  const std::string cid = "model/smm/bc_functors_" + tname.substr(1);
  ap::FixtureWriter w(g_root, cid,
                      "Dirichlet and Neumann functors applied on boundary_0 of " + tname,
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", data / "material_check_stress_plane_strain.dat",
                   "upstream:test/test_model/patch_tests/data/"
                   "material_check_stress_plane_strain.dat");
  w.parameters()["spatial_dimension"] = static_cast<long long>(dim);
  w.parameters()["element_type"] = tname;
  w.parameters()["model_id"] = tname;
  const std::string g = "boundary_0";
  auto rec = [&](const std::string & stage) {
    add_array(w, stage + "/displacement", model.getDisplacement(), "displacement");
    add_bools(w, stage + "/blocked_dofs", model.getBlockedDOFs(), "blocked flags");
    add_array(w, stage + "/external_force", model.getExternalForce(), "external force");
  };
  model.applyBC(BC::Dirichlet::FixedValue(0.01, _x), g);
  rec("fixed_value_x");
  model.applyBC(BC::Dirichlet::IncrementValue(0.005, _y), g);
  model.applyBC(BC::Dirichlet::IncrementValue(0.005, _y), g);
  rec("increment_value_y_twice");
  Vector<Real> inc(dim);
  for (Int i = 0; i < dim; ++i) {
    inc(i) = 1e-3 * (i + 1);
  }
  model.applyBC(BC::Dirichlet::Increment(inc), g);
  rec("increment_vector");
  model.getBlockedDOFs().zero();
  model.applyBC(BC::Dirichlet::FlagOnly(dim > 1 ? _y : _x), g);
  rec("flag_only");
  Vector<Real> t(dim);
  for (Int i = 0; i < dim; ++i) {
    t(i) = 1e5 * (i + 1) - 2.5e4;
  }
  if (dim > 1) {
    model.applyBC(BC::Neumann::FromSameDim(t), g);
    rec("from_same_dim");
    Matrix<Real> sigma(dim, dim);
    for (Int i = 0; i < dim; ++i) {
      for (Int j = 0; j < dim; ++j) {
        sigma(i, j) = 1e6 * (1 + i) + 3e5 * j; // non-symmetric: detects a transpose
      }
    }
    model.applyBC(BC::Neumann::FromHigherDim(sigma), g);
    rec("from_higher_dim");
    model.applyBC(BC::Neumann::FreeBoundary(), g);
    rec("free_boundary");
  }
  record_mesh(w, mesh);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* review adversarial cases */
/* -------------------------------------------------------------------------- */
const char * kReviewMesh = R"MSH($MeshFormat
2.2 0 8
$EndMeshFormat
$PhysicalNames
5
3 1 "steel"
3 2 "alu"
2 3 "left"
2 4 "right"
2 5 "top"
$EndPhysicalNames
$Nodes
36
1 0.0 0.0 0.0
2 0.9375 0.0 0.0
3 1.9375 0.0 0.0
4 3.0 0.0 0.0
5 0.0 0.5 0.0
6 1.0625 0.5 0.0
7 2.0625 0.5 0.0
8 3.0 0.5 0.0
9 0.0 1.0 0.0
10 0.9375 1.0 0.0
11 1.9375 1.0 0.0
12 3.0 1.0 0.0
13 0.0 0.0 0.5
14 1.0625 0.0 0.5
15 2.0625 0.0 0.5
16 3.0 0.0 0.5
17 0.0 0.5 0.5
18 1.125 0.4375 0.53125
19 2.125 0.4375 0.53125
20 3.0 0.5 0.5
21 0.0 1.0 0.5
22 1.0625 1.0 0.5
23 2.0625 1.0 0.5
24 3.0 1.0 0.5
25 0.0 0.0 1.0
26 0.9375 0.0 1.0
27 1.9375 0.0 1.0
28 3.0 0.0 1.0
29 0.0 0.5 1.0
30 1.0625 0.5 1.0
31 2.0625 0.5 1.0
32 3.0 0.5 1.0
33 0.0 1.0 1.0
34 0.9375 1.0 1.0
35 1.9375 1.0 1.0
36 3.0 1.0 1.0
$EndNodes
$Elements
26
1 5 2 1 1 1 2 6 5 13 14 18 17
2 5 2 2 2 2 3 7 6 14 15 19 18
3 5 2 1 1 3 4 8 7 15 16 20 19
4 5 2 2 2 5 6 10 9 17 18 22 21
5 5 2 1 1 6 7 11 10 18 19 23 22
6 5 2 2 2 7 8 12 11 19 20 24 23
7 5 2 2 2 13 14 18 17 25 26 30 29
8 5 2 1 1 14 15 19 18 26 27 31 30
9 5 2 2 2 15 16 20 19 27 28 32 31
10 5 2 1 1 17 18 22 21 29 30 34 33
11 5 2 2 2 18 19 23 22 30 31 35 34
12 5 2 1 1 19 20 24 23 31 32 36 35
13 3 2 3 3 1 13 17 5
14 3 2 4 4 4 8 20 16
15 3 2 3 3 5 17 21 9
16 3 2 4 4 8 12 24 20
17 3 2 3 3 13 25 29 17
18 3 2 4 4 16 20 32 28
19 3 2 3 3 17 29 33 21
20 3 2 4 4 20 24 36 32
21 3 2 5 5 25 26 30 29
22 3 2 5 5 26 27 31 30
23 3 2 5 5 27 28 32 31
24 3 2 5 5 29 30 34 33
25 3 2 5 5 30 31 35 34
26 3 2 5 5 31 32 36 35
$EndElements
)MSH";
const char * kReviewDat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n]\n"
                          "material elastic [\n  name = alu\n  rho = 2700\n  E = 7e10\n  nu = 0.33\n]\n";

fs::path write_tmp(const std::string & name, const std::string & text) {
  auto p = fs::temp_directory_path() / name;
  std::ofstream f(p, std::ios::binary | std::ios::trunc);
  f << text;
  return p;
}

void record_index_all(ap::FixtureWriter & w, SolidMechanicsModel & model, const std::string & prefix) {
  const auto & mesh = model.getMesh();
  {
    Json present = Json::array();
    for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
      if (model.getMaterialByElement().exists(type, _not_ghost)) present.push(std::to_string(type));
    }
    w.parameters()[prefix + "index_map_types"] = present;
  }
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    if (not model.getMaterialByElement().exists(type, _not_ghost)) continue;
    const auto & idx = model.getMaterialByElement(type);
    const auto & loc = model.getMaterialLocalNumbering(type);
    std::vector<I64> a(idx.data(), idx.data() + idx.size());
    std::vector<I64> b(loc.data(), loc.data() + loc.size());
    add_ivec(w, prefix + "material_index/" + std::to_string(type), a, "material per element (all types)");
    add_ivec(w, prefix + "local_numbering/" + std::to_string(type), b, "law-local index (all types)");
  }
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    const auto & mat = static_cast<const Material &>(model.getMaterial(m));
    for (auto type : mat.getElementFilter().elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
      const auto & f = mat.getElementFilter(type);
      std::vector<I64> v(f.data(), f.data() + f.size());
      add_ivec(w, prefix + "filter/" + std::to_string(m) + "/" + std::to_string(type), v, "filter (all types)");
    }
  }
}

void review_mixed_static() {
  if (not wanted("model/smm/review_mixed_static")) {
    return;
  }
  auto mp = write_tmp("review_checker.msh", kReviewMesh);
  auto dp = write_tmp("review_two.dat", kReviewDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(3);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  auto sel = std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  model.setMaterialSelector(sel);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 5);
  solver.set("threshold", 1e-10);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _z), "left");
  Matrix<Real> sigma(3, 3);
  for (Int i = 0; i < 3; ++i)
    for (Int j = 0; j < 3; ++j)
      sigma(i, j) = 1e6 * (1 + i) + 3e5 * j - 2e5 * (i == j);
  model.applyBC(BC::Neumann::FromHigherDim(sigma), "right");
  Vector<Real> t{1e5, -2e5, -3e5};
  model.applyBC(BC::Neumann::FromSameDim(t), "top");
  ap::FixtureWriter w(g_root, "model/smm/review_mixed_static",
                      "review V41 adversarial: checkerboard steel/alu hex8 mesh with boundary quad4 "
                      "groups, physical-name selector, FromHigherDim / FromSameDim loads, static "
                      "Newton-Raphson (Eigen sparse solver)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", mp, "generated:review");
  w.add_input_file("material", dp, "generated:review");
  w.parameters()["spatial_dimension"] = 3LL;
  add_array(w, "external_force_before", model.getExternalForce(), "f_ext after Neumann");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "blocked");
  record_index_all(w, model, "before/");
  model.solveStep();
  add_ivec(w, "newton/nb_iterations", {Int(solver.get("nb_iterations"))}, "its");
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "err");
  add_array(w, "displacement", model.getDisplacement(), "u");
  add_array(w, "internal_force", model.getInternalForce(), "f_int");
  record_internals(w, model, 0, "mat0/");
  record_internals(w, model, 1, "mat1/");
  record_aij(w, "K", model, "K");
  add_vec(w, "energy/potential", {model.getEnergy("potential")}, "Epot");
  add_vec(w, "energy/external_work", {model.getEnergy("external work")}, "Wext");
  // getEnergy("potential", group) is not recorded: upstream integrates an
  // uninitialised vector per element (UD-032; 1.04e228 was measured)
  w.finish();
  std::cout << "  model/smm/review_mixed_static" << std::endl;
}

void review_consistent_blocked_change() {
  if (not wanted("model/smm/review_consistent_blocked_change")) {
    return;
  }
  auto mp = write_tmp("review_checker.msh", kReviewMesh);
  auto dp = write_tmp("review_two.dat", kReviewDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(3);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  auto sel = std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  model.setMaterialSelector(sel);
  model.initFull(_analysis_method = _explicit_consistent_mass, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 20);
  solver.set("threshold", 1e-10);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  block_xmin(model);
  linear_velocity(model);
  const Real stable = model.getStableTimeStep();
  model.setTimeStep(0.5 * stable);
  ap::FixtureWriter w(g_root, "model/smm/review_consistent_blocked_change",
                      "review V41 adversarial: explicit consistent mass on the checkerboard mesh, "
                      "blocked DOFs and nodal forces changed after step 8 (Eigen sparse solver)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", mp, "generated:review");
  w.add_input_file("material", dp, "generated:review");
  w.parameters()["spatial_dimension"] = 3LL;
  add_vec(w, "time_step", {stable, 0.5 * stable}, "dt");
  std::vector<double> ek, ep, ew, ewn, u_all, v_all, a_all, errs;
  std::vector<I64> its;
  auto snap = [&]() {
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
    ew.push_back(model.getEnergy("external work"));
    ewn.push_back(model.getEnergy("external work new"));
    for (auto x : vec_of(model.getDisplacement())) u_all.push_back(x);
    for (auto x : vec_of(model.getVelocity())) v_all.push_back(x);
    for (auto x : vec_of(model.getAcceleration())) a_all.push_back(x);
  };
  snap();
  auto & b = model.getBlockedDOFs();
  const auto & X = mesh.getNodes();
  for (Int s = 0; s < 16; ++s) {
    if (s == 8) {
      // unblock z on x = xmin, block y on x = xmax (nodes keep their displacement)
      for (Int n = 0; n < X.size(); ++n) {
        if (std::abs(X(n, 0)) < 1e-10) b(n, 2) = false;
        if (std::abs(X(n, 0) - 3.) < 1e-10) b(n, 1) = true;
      }
      // and a nodal external force
      model.getExternalForce()(5, 0) = 1e7;
      model.getExternalForce()(30, 2) = -2e7;
    }
    model.solveStep();
    its.push_back(Int(solver.get("nb_iterations")));
    errs.push_back(Real(solver.get("error")));
    snap();
  }
  const auto nn = static_cast<std::size_t>(mesh.getNbNodes());
  for (auto && [name, v] : std::vector<std::pair<std::string, std::vector<double> *>>{
           {"displacement", &u_all}, {"velocity", &v_all}, {"acceleration", &a_all}}) {
    w.add(name, {17, nn, 3}, {"step", "node", "component"}, "nodal", *v).description("states");
  }
  add_vec(w, "energy/kinetic", ek, "");
  add_vec(w, "energy/potential", ep, "");
  add_vec(w, "energy/external_work", ew, "");
  add_vec(w, "energy/external_work_new", ewn, "");
  add_ivec(w, "newton/nb_iterations", its, "");
  add_vec(w, "newton/error", errs, "");
  add_bools(w, "blocked_dofs_final", b, "");
  add_array(w, "internal_force", model.getInternalForce(), "");
  record_aij(w, "M", model, "M");
  record_aij(w, "K", model, "K", false);
  record_index_all(w, model, "");
  w.finish();
  std::cout << "  model/smm/review_consistent_blocked_change" << std::endl;
}


/* -------------------------------------------------------------------------- */
/// fixtures/model/static/loaded_<type> (model_probe.py run_loaded) with the
/// Eigen sparse solver: x = xmin face blocked, nodal force on the x = xmax
/// face, Newton-Raphson residual criterion (review optional twins).
void loaded_case(ElementType type) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  const std::string cid = "model/smm/loaded_static_" + tname.substr(1);
  if (not wanted(cid)) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  auto dp = write_tmp("smm_probe_loaded.dat",
                      "material elastic [\n  name = elastic\n  rho = 7800.\n  E = 2.1e11\n"
                      "  nu = 0.3\n]\n");
  getStaticParser().parse(dp.string());
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  const auto & X = mesh.getNodes();
  Real xmin = X(0, 0), xmax = X(0, 0), amax = 0.;
  for (Int n = 0; n < X.size(); ++n) {
    xmin = std::min(xmin, X(n, 0));
    xmax = std::max(xmax, X(n, 0));
    for (Int c = 0; c < dim; ++c) {
      amax = std::max(amax, std::abs(X(n, c)));
    }
  }
  const Real tol = 1e-10 * std::max(1., amax);
  const Real load[3] = {2.0e5, -1.0e5, 5.0e4};
  auto & blocked = model.getBlockedDOFs();
  auto & fext = model.getExternalForce();
  for (Int n = 0; n < X.size(); ++n) {
    for (Int c = 0; c < dim; ++c) {
      if (std::abs(X(n, 0) - xmin) < tol) {
        blocked(n, c) = true;
      }
      if (std::abs(X(n, 0) - xmax) < tol) {
        fext(n, c) = load[c];
      }
    }
  }
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 10);
  solver.set("threshold", 1e-12);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  model.solveStep();
  ap::FixtureWriter w(g_root, cid,
                      "fixtures/model/static/loaded_" + tname.substr(1) +
                          " (model_probe.py run_loaded) with the Eigen sparse solver",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", dp, "generated:smm_probe loaded_case");
  w.parameters()["spatial_dimension"] = static_cast<long long>(dim);
  w.parameters()["element_type"] = tname;
  add_vec(w, "nodal_load", std::vector<double>(load, load + dim), "load per x = xmax node");
  add_bools(w, "blocked_dofs", blocked, "x = xmin face blocked");
  add_array(w, "external_force", fext, "nodal load on the x = xmax face");
  add_ivec(w, "newton/nb_iterations", {Int(solver.get("nb_iterations"))}, "nb_iterations");
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "error");
  add_array(w, "displacement", model.getDisplacement(), "solution");
  add_array(w, "internal_force", model.getInternalForce(), "internal force");
  record_internals(w, model, 0);
  record_aij(w, "K", model, "K");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

} // namespace

/// D-050 pins of the v2 cases (recorded in their parameters).
ap::Json oracle_pins() {
  ap::Json j = ap::Json::object();
  j["eigen_cpu_cache_sizes"] = "setCpuCacheSizes(16384, 524288, 524288) at the start of main";
  const char * ct = std::getenv("OPENBLAS_CORETYPE");
  const char * nt = std::getenv("OPENBLAS_NUM_THREADS");
  j["OPENBLAS_CORETYPE"] = ct != nullptr ? ct : "";
  j["OPENBLAS_NUM_THREADS"] = nt != nullptr ? nt : "";
  return j;
}

/* -------------------------------------------------------------------------- */
/* v2: kinetic energy before the first explicit lumped step (item 5)           */
/* -------------------------------------------------------------------------- */
void kinetic_before_step_case(ElementType type) {
  const auto dim = Mesh::getSpatialDimension(type);
  const auto tname = std::to_string(type);
  const std::string cid = "model/smm/explicit_lumped_kinetic_" + tname.substr(1);
  if (not wanted(cid)) {
    return;
  }
  const auto data = g_up / "test/test_model/patch_tests/data";
  const std::string dat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n"
                          "  nu = 0.3\n]\n";
  auto tmp = fs::temp_directory_path() / "smm_probe_kinetic.dat";
  {
    std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
    f << dat;
  }
  getStaticParser().parse(tmp.string());
  Mesh mesh(dim);
  mesh.read((data / (tname + ".msh")).string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  block_xmin(model);
  linear_velocity(model);
  ap::FixtureWriter w(g_root, cid,
                      "explicit lumped mass on " + tname +
                          ": matrices after initFull, kinetic energy before and after the "
                          "first step (x = xmin blocked, linear initial velocity)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", data / (tname + ".msh"),
                   "upstream:test/test_model/patch_tests/data/" + tname + ".msh");
  w.add_input_file("material", tmp, "generated:smm_probe.cc");
  fs::remove(tmp);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["element_type"] = tname;
  p["oracle_pins"] = oracle_pins();
  auto & dm = model.getDOFManager();
  auto flags = [&](const std::string & stage) {
    for (const auto * id : {"K", "M", "C", "J"}) {
      p[stage + "/has_matrix_" + id] = dm.hasMatrix(id);
    }
    p[stage + "/has_lumped_matrix_M"] = dm.hasLumpedMatrix("M");
  };
  flags("after_init_full");
  std::vector<double> ek;
  ek.push_back(model.getEnergy("kinetic"));
  flags("after_kinetic_0");
  record_aij(w, "M_after_kinetic_0", model, "M");
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.5 * stable;
  model.setTimeStep(dt);
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
  model.solveStep();
  flags("after_step_1");
  ek.push_back(model.getEnergy("kinetic"));
  add_vec(w, "energy/kinetic", ek,
          "getEnergy(\"kinetic\") before the first step and after it");
  add_array(w, "velocity", model.getVelocity(),
            "velocity after the step");
  add_array(w, "displacement", model.getDisplacement(), "displacement after the step");
  add_array(w, "mass", model.getMass(), "lumped mass (model array) after the step");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* v2: periodic mesh + boundary-condition functors */
/* -------------------------------------------------------------------------- */
void periodic_case(Int dim, Int dim_periodic) {
  const std::string cid = "model/smm/periodic_static_" + std::to_string(dim) + "d_" +
                          std::to_string(dim_periodic);
  if (not wanted(cid)) {
    return;
  }
  const std::string rel = "test/test_mesh_utils/test_mesh_periodic/";
  const std::string mname = dim == 1 ? "line.msh" : (dim == 2 ? "square.msh" : "cube.msh");
  const auto mesh_file = g_frozen / rel / mname;
  const auto mat_file = g_up / rel / "material.dat";
  getStaticParser().parse(mat_file.string());
  Mesh mesh(dim);
  mesh.read(mesh_file.string());
  mesh.distribute();
  const SpatialDirection dirs[3] = {_x, _y, _z};
  for (Int d = 0; d < dim_periodic; ++d) {
    mesh.makePeriodic(dirs[d]);
  }
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  ap::FixtureWriter w(g_root, cid,
                      "TestPeriodicStatic (tensile) on the frozen " + mname + ", periodic in " +
                          std::to_string(dim_periodic) +
                          " direction(s): BC functors, then the static solve (Eigen)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm");
  w.add_input_file("mesh", mesh_file, "frozen:" + rel + mname);
  w.add_input_file("material", mat_file, "upstream:" + rel + "material.dat");
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["periodic_directions"] = static_cast<long long>(dim_periodic);
  p["is_periodic"] = mesh.isPeriodic();
  p["oracle_pins"] = oracle_pins();
  {
    std::vector<I64> slaves, masters;
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      if (mesh.isPeriodicSlave(n)) {
        slaves.push_back(n);
        masters.push_back(mesh.getPeriodicMaster(n));
      }
    }
    add_ivec(w, "periodic/slaves", slaves, "periodic slave nodes (increasing)");
    add_ivec(w, "periodic/masters", masters, "getPeriodicMaster(slave)");
  }
  auto rec = [&](const std::string & stage) {
    add_array(w, stage + "/displacement", model.getDisplacement(), "displacement");
    add_bools(w, stage + "/blocked_dofs", model.getBlockedDOFs(), "blocked flags");
    add_array(w, stage + "/external_force", model.getExternalForce(), "external force");
  };
  for (Int d = 0; d < dim; ++d) {
    model.applyBC(BC::Dirichlet::FixedValue(0.0, dirs[d]), "corner");
  }
  rec("corner_fixed");
  Vector<Real> traction(dim);
  traction.fill(0.);
  traction(dim - 1) = 1e9;
  if (dim == 1) {
    for (auto && node : mesh.getElementGroup("x-plus").getNodeGroup().getNodes()) {
      model.getExternalForce()(node) = traction(_x);
    }
    rec("nodal_load");
    mesh.makeConsistentForPeriodicity(model.getExternalForce());
    rec("nodal_load_consistent");
    // BC::Neumann functors are not implemented in 1-D (AKANTU_TO_IMPLEMENT)
    std::string message;
    try {
      model.applyBC(BC::Neumann::FromTraction(traction), "x-plus");
    } catch (debug::Exception & e) {
      message = e.what();
    }
    p["neumann_1d_error"] = message;
    rec("after_neumann_1d");
  } else if (dim == 2) {
    model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "y-minus");
    rec("y_minus_fixed");
    model.applyBC(BC::Neumann::FromTraction(traction), "y-plus");
    rec("traction");
  } else {
    model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "line");
    rec("line_fixed");
    model.applyBC(BC::Dirichlet::FixedValue(0.0, _z), "z-minus");
    rec("z_minus_fixed");
    model.applyBC(BC::Neumann::FromTraction(traction), "z-plus");
    rec("traction");
  }
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 5);
  solver.set("threshold", 2e-4);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);
  std::string solve_error;
  try {
    model.solveStep();
  } catch (debug::Exception & e) {
    solve_error = e.what();
  }
  p["solve_error"] = solve_error;
  p["newton/nb_iterations"] = static_cast<long long>(Int(solver.get("nb_iterations")));
  p["newton/converged"] = bool(solver.get("converged"));
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "NonLinearSolver error");
  rec("solved");
  add_array(w, "solved/internal_force", model.getInternalForce(), "internal force");
  record_internals(w, model, 0);
  record_aij(w, "K", model, "K");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: smm_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  // D-050: Eigen's own default cache sizes (host-independent blocking)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    const std::vector<ElementType> all = {
        _segment_2,    _segment_3,      _triangle_3,    _triangle_6,
        _quadrangle_4, _quadrangle_8,   _tetrahedron_4, _tetrahedron_10,
        _hexahedron_8, _hexahedron_20,  _pentahedron_6, _pentahedron_15};
    const std::vector<std::tuple<AnalysisMethod, std::string>> analyses = {
        {_static, "static"}, {_explicit_lumped_mass, "explicit"}, {_implicit_dynamic, "implicit"}};
    for (auto type : all) {
      const auto dim = Mesh::getSpatialDimension(type);
      for (auto && [m, a] : analyses) {
        patch_case({type, m, a, "elastic", "material_check_stress_plane_strain.dat",
                    dim == 2 ? "plane_strain" : ""});
        if (dim == 2) {
          patch_case({type, m, a, "elastic", "material_check_stress_plane_stress.dat",
                      "plane_stress"});
        }
        if (m != _static) {
          patch_case({type, m, a, "anisotropic",
                      "material_anisotropic_" + std::to_string(dim) + ".dat", ""});
        }
      }
      lumped_mass_case(type);
    }
    for (auto type : {_triangle_3, _quadrangle_4, _tetrahedron_4, _hexahedron_8}) {
      for (auto && [m, a] : std::vector<std::tuple<AnalysisMethod, std::string>>{
               {_explicit_lumped_mass, "explicit"}, {_implicit_dynamic, "implicit"}}) {
        patch_case({type, m, a, "fd", "material_check_stress_plane_strain_finite_deformation.dat",
                    Mesh::getSpatialDimension(type) == 2 ? "plane_strain" : ""});
      }
    }
    for (auto type : all) {
      const auto dim = Mesh::getSpatialDimension(type);
      PatchSpec spec{type, _static, "static", "anisotropic",
                     "material_anisotropic_" + std::to_string(dim) + ".dat", ""};
      spec.anisotropic_static = true;
      patch_case(spec);
    }
    eigenstrain_case();
    eigenstrain_case(true);
    reassign_case();
    for (auto type : {_triangle_3, _quadrangle_4, _tetrahedron_10, _hexahedron_8}) {
      dynamic_case(type, _explicit_consistent_mass, "consistent_explicit", 20);
      dynamic_case(type, _implicit_dynamic, "implicit_dynamic", 10);
    }
    for (auto type : {_quadrangle_4, _hexahedron_8, _triangle_6}) {
      bc_case(type);
    }
    review_mixed_static();
    review_consistent_blocked_change();
    for (auto type : {_quadrangle_4, _tetrahedron_4}) {
      loaded_case(type);
    }
    // v2
    for (auto type : {_triangle_3, _hexahedron_8}) {
      kinetic_before_step_case(type);
    }
    for (auto && [dim, dp] : std::vector<std::pair<Int, Int>>{{1, 1}, {2, 1}, {3, 1}, {3, 2}}) {
      periodic_case(dim, dp);
    }
  } catch (std::exception & e) {
    std::cerr << "smm_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
