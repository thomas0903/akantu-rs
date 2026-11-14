// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the upstream StructuralMechanicsModel
// (src/model/structural_mechanics) on the set-ups of the upstream tests
// test/test_model/test_structural_mechanics_model/* and on generated cases
// (line loads, explicit dynamics, lumped mass, a mixed beam/shell frame),
// with the Eigen sparse solver pinned (D-035), and records the assembled
// matrices (COO, storage order), the states and the energies for the Rust
// differential tests (fixtures/model/structural/*).
//
// Version 2: the adversarial cases (v76_*:
// 3-D frame in general orientation with theta_y distributed moments, tilted
// DKT plates, a tilted plate with edge beams, implicit and explicit-
// consistent 3-D beam dynamics with torsion, an inclined 2-D chain, and the
// kinetic energy of an explicit-lumped analysis before the first step,
// UD-510), and Eigen's cache sizes pinned at the start of main (D-050:
// Eigen::setCpuCacheSizes(16384, 524288, 524288), the aarch64 defaults the
// port's blocking reproduces; recorded in every manifest's parameters and
// notes). The 12 cases of version 1 are unchanged (regenerated
// byte-identical without the pin; their fronts are far below the blocking
// thresholds).
//
// Version 3: the case
// dump_set_time_step (STRUCT-013): the model's default ParaView dumper
// `structural_mechanics_model` and an extra text-mode dumper registered
// before setTimeStep; StructuralMechanicsModel::setTimeStep sets only the
// mesh's default dumper (structural_mechanics_model.cc:130-134), so only its
// time description (.pvd) is written; every dumped file is recorded byte for
// byte (inputs role "dump:<path relative to the dump directory>"). The cases
// of versions 1 and 2 are unchanged.
//
// Recorded per case (raw Akantu arrays, [entry, component] tables):
//   inputs           nodes, connectivity, extra_normal, materials,
//                    element_material, blocked_dofs, imposed displacement,
//                    external_force (after the line loads), tractions,
//                    initial velocity (v76 dynamic cases)
//   K/*, M/*         SparseMatrixAIJ COO (getIRN-1, getJCN-1, getA)
//   final/*          displacement, internal_force, stress per type,
//                    velocity / acceleration (dynamic)
//   energy/*         getEnergy("kinetic"|"potential")
//   series/*         per step (dynamic): node displacement, energies,
//                    newton iterations; full displacement at a few steps
//   lumped/*         getLumpedMass after assembleLumpedMassMatrix (upstream
//                    UD-005: identically zero)
//   pre/*            (v76 *_lumped_pre_energy) matrices that exist after
//                    initFull, getEnergy("kinetic") before any step, the
//                    consistent M, the energy after the lumped assembly
//
// Usage: structural_model_probe <fixtures_root> <upstream_src_root>
// STRUCT_MODEL_PROBE_CASES=<id,id,...> restricts the cases.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <dof_manager.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <non_linear_solver.hh>
#include <sparse_matrix_aij.hh>
#include <structural_mechanics_model.hh>
// dumper headers after mesh.hh (as oracle/probes/dump/dump_probe.cc)
#include <dumpable_inline_impl.hh>
#include <dumper_iohelper_paraview.hh>

// after the Akantu headers: Akantu defines its Eigen plugins before the
// first Eigen include (Eigen::setCpuCacheSizes, D-050)
#include <Eigen/Core>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <functional>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;
using I64 = std::int64_t;
using Json = ap::Json;

namespace {

const char * const kProbe = "oracle/probes/structural_model/structural_model_probe.cc";
const char * const kProbeVersion = "3";
fs::path g_root;
fs::path g_up;
const char * const kTests = "test/test_model/test_structural_mechanics_model/";

bool wanted(const std::string & id) {
  const char * env = std::getenv("STRUCT_MODEL_PROBE_CASES");
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

void add_bools(ap::FixtureWriter & w, const std::string & name, const Array<bool> & a,
               const std::string & desc) {
  std::vector<bool> v(a.data(), a.data() + a.size() * a.getNbComponent());
  w.add_bool(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
             {"entry", "component"}, "table", v)
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

/// Deterministic input values (no RNG dependency).
Real pseudo(Int i, Int j, Real scale) {
  return scale * std::sin(0.7 * Real(i) + 1.3 * Real(j) + 0.25);
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

/// COO of an AIJ matrix (0-based rows/cols, storage order).
void record_aij(ap::FixtureWriter & w, const std::string & prefix, StructuralMechanicsModel & model,
                const ID & id) {
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
  add_ivec(w, prefix + "/rows", rows, "SparseMatrixAIJ::getIRN - 1 (storage order)");
  add_ivec(w, prefix + "/cols", cols, "SparseMatrixAIJ::getJCN - 1 (storage order)");
  add_vec(w, prefix + "/values", vals, "SparseMatrixAIJ::getA (storage order)");
}

/// A structural case: mesh (upstream file or explicit), materials, method,
/// a set-up callback (BCs, loads, solver parameters) and a step count.
struct Case {
  std::string id;
  std::string description;
  Int dim;
  std::string mesh_file;                       // upstream path or ""
  std::vector<std::vector<Real>> nodes;        // explicit meshes
  std::vector<std::pair<ElementType, std::vector<std::vector<Idx>>>> blocks;
  std::vector<std::pair<ElementType, std::vector<std::vector<Real>>>> normals;
  std::vector<StructuralMaterial> materials;
  std::vector<std::pair<ElementType, std::vector<UInt>>> element_material;
  AnalysisMethod method;
  std::function<void(StructuralMechanicsModel &, ap::FixtureWriter &)> setup;
  Int nb_steps{1};
  Real time_step{0.};
  Idx series_node{-1};
  Int series_dof{0};
  std::vector<Int> full_steps{};
  bool lumped_only{false};
  bool pre_energy{false};   // v76: kinetic energy before any step / assembly
  std::vector<Real> initial_velocity_scale{}; // v76: initial velocity (pseudo) per dof
  std::string upstream_test;
};

void record_inputs(ap::FixtureWriter & w, const Case & c, Mesh & mesh) {
  add_array(w, "mesh/nodes", mesh.getNodes(), "input: node coordinates");
  Json types = Json::array();
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_structural)) {
    const auto & conn = mesh.getConnectivity(type);
    std::vector<I64> v(conn.data(), conn.data() + conn.size() * conn.getNbComponent());
    auto name = std::to_string(type);
    types.push(name);
    w.add("mesh/connectivity/" + name,
          {static_cast<std::size_t>(conn.size()), static_cast<std::size_t>(conn.getNbComponent())},
          {"element", "node_local"}, "elemental", v)
        .element_type(name)
        .description("input: connectivity");
  }
  w.parameters()["mesh_types"] = types;
  {
    std::vector<double> m;
    for (auto && mt : c.materials) {
      m.insert(m.end(), {mt.E, mt.A, mt.I, mt.Iz, mt.Iy, mt.GJ, mt.rho, mt.t, mt.nu});
    }
    w.add("materials", {c.materials.size(), 9}, {"entry", "component"}, "table", m)
        .components({"E", "A", "I", "Iz", "Iy", "GJ", "rho", "t", "nu"})
        .description("input: StructuralMaterial list (addMaterial order)");
  }
}

template <typename T> std::vector<I64> ivec(const Array<T> & a) {
  return std::vector<I64>(a.data(), a.data() + a.size() * a.getNbComponent());
}

void record_state(ap::FixtureWriter & w, const std::string & prefix,
                  StructuralMechanicsModel & model, bool dynamic) {
  add_array(w, prefix + "/displacement", model.getDisplacement(),
            "displacement and rotations (nb_degree_of_freedom per node)");
  add_array(w, prefix + "/internal_force", model.getInternalForce(), "internal force");
  if (dynamic) {
    add_array(w, prefix + "/velocity", model.getVelocity(), "velocity");
    add_array(w, prefix + "/acceleration", model.getAcceleration(), "acceleration");
  }
  for (auto type : model.getMesh().elementTypes(model.getSpatialDimension(), _not_ghost,
                                                _ek_structural)) {
    add_array(w, prefix + "/stress/" + std::to_string(type), model.getStress(type),
              "generalised stresses per quadrature point");
  }
}

void run(const Case & c) {
  if (not wanted(c.id)) {
    return;
  }
  const auto cid = std::string("model/structural/") + c.id;
  Mesh mesh(c.dim);
  if (not c.mesh_file.empty()) {
    mesh.read((g_up / c.mesh_file).string(), _miot_gmsh_struct);
  } else {
    MeshAccessor acc(mesh);
    auto & nodes = acc.getNodes();
    nodes.resize(c.nodes.size());
    for (std::size_t i = 0; i < c.nodes.size(); ++i) {
      for (Int d = 0; d < c.dim; ++d) {
        nodes(i, d) = c.nodes[i][d];
      }
    }
    for (auto && [type, rows] : c.blocks) {
      mesh.addConnectivityType(type);
      auto & conn = acc.getConnectivity(type);
      conn.resize(rows.size());
      for (std::size_t e = 0; e < rows.size(); ++e) {
        for (std::size_t k = 0; k < rows[e].size(); ++k) {
          conn(e, k) = rows[e][k];
        }
      }
    }
    acc.makeReady();
  }

  ap::FixtureWriter w(g_root, cid, c.description, kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("structural");
  if (not c.mesh_file.empty()) {
    w.add_input_file("mesh", g_up / c.mesh_file, "upstream:" + c.mesh_file);
  }
  if (not c.upstream_test.empty()) {
    w.parameters()["upstream_test"] = c.upstream_test;
  }
  w.parameters()["spatial_dimension"] = static_cast<long long>(c.dim);
  w.parameters()["analysis_method"] = std::to_string(c.method);
  w.parameters()["sparse_solver"] = "eigen";
  // D-050: Eigen cache sizes pinned in main (read back here)
  {
    Json cache = Json::array();
    cache.push(static_cast<long long>(Eigen::l1CacheSize()));
    cache.push(static_cast<long long>(Eigen::l2CacheSize()));
    cache.push(static_cast<long long>(Eigen::l3CacheSize()));
    w.parameters()["eigen_cpu_cache_sizes"] = cache;
  }
  w.set_notes("D-050: Eigen::setCpuCacheSizes(16384, 524288, 524288) at the start of the "
              "probe's main (host-independent blocking); parameters/eigen_cpu_cache_sizes holds "
              "the values read back (Eigen::l1CacheSize, l2CacheSize, l3CacheSize)");

  StructuralMechanicsModel model(mesh, _all_dimensions, c.id);
  for (auto && [type, rows] : c.normals) {
    auto & n = mesh.getData<Real>("extra_normal", type);
    for (std::size_t e = 0; e < rows.size(); ++e) {
      for (Int d = 0; d < c.dim; ++d) {
        n(e, d) = rows[e][d];
      }
    }
    add_array(w, "extra_normal/" + std::to_string(type), n, "input: extra_normal");
  }
  for (auto mat : c.materials) {
    model.addMaterial(mat);
  }
  model.initFull(_analysis_method = c.method, _solver_options = eigen_options());
  for (auto && [type, mats] : c.element_material) {
    auto & em = model.getElementMaterial(type);
    for (std::size_t e = 0; e < mats.size(); ++e) {
      em(e) = mats[e];
    }
  }
  record_inputs(w, c, mesh);
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_structural)) {
    add_ivec(w, "element_material/" + std::to_string(type), ivec(model.getElementMaterial(type)),
             "input: material index per element");
    w.parameters()["nb_quadrature_points/" + std::to_string(type)] =
        static_cast<long long>(model.getFEEngine().getNbIntegrationPoints(type));
  }
  if (c.setup) {
    c.setup(model, w);
  }

  if (c.pre_energy) {
    // v76 (UD-510): which mass matrices exist after initFull, and the kinetic
    // energy before any step or explicit mass assembly.
    auto & dm = model.getDOFManager();
    w.parameters()["pre/has_matrix_M"] = dm.hasMatrix("M");
    w.parameters()["pre/has_matrix_K"] = dm.hasMatrix("K");
    w.parameters()["pre/has_matrix_C"] = dm.hasMatrix("C");
    w.parameters()["pre/has_lumped_M"] = dm.hasLumpedMatrix("M");
    auto & v = model.getVelocity();
    for (Idx i = 0; i < v.size(); ++i) {
      for (Int d = 0; d < v.getNbComponent(); ++d) {
        v(i, d) = pseudo(i, d, 0.5);
      }
    }
    add_array(w, "pre/velocity", v, "input: velocity before any step");
    add_vec(w, "pre/kinetic_energy", {model.getEnergy("kinetic")},
            "getEnergy(\"kinetic\") right after initFull (no step, no explicit assembly)");
    w.parameters()["pre/has_lumped_M_after"] = dm.hasLumpedMatrix("M");
    record_aij(w, "pre/M", model, "M");
    model.assembleLumpedMassMatrix();
    add_vec(w, "pre/kinetic_energy_after_lumped", {model.getEnergy("kinetic")},
            "getEnergy(\"kinetic\") after assembleLumpedMassMatrix");
    w.finish();
    return;
  }

  if (c.lumped_only) {
    model.assembleLumpedMassMatrix();
    add_array(w, "lumped/mass", model.getLumpedMass(),
              "getLumpedMass after assembleLumpedMassMatrix (UD-005: upstream uses the first "
              "node twice, so every value is zero)");
    auto & v = model.getVelocity();
    for (Idx i = 0; i < v.size(); ++i) {
      for (Int d = 0; d < v.getNbComponent(); ++d) {
        v(i, d) = pseudo(i, d, 0.5);
      }
    }
    add_array(w, "lumped/velocity", v, "input: velocity for the lumped kinetic energy");
    add_vec(w, "lumped/kinetic_energy", {model.getEnergy("kinetic")},
            "getEnergy(\"kinetic\") with the lumped mass");
    w.finish();
    return;
  }

  add_bools(w, "input/blocked_dofs", model.getBlockedDOFs(), "input: blocked DOFs");
  add_array(w, "input/displacement", model.getDisplacement(),
            "input: displacement before the first step (imposed values)");
  add_array(w, "input/external_force", model.getExternalForce(),
            "input: external force (after the line loads, if any)");

  const bool dynamic = c.method != _static;
  if (c.time_step > 0) {
    model.setTimeStep(c.time_step);
    w.parameters()["time_step"] = c.time_step;
    // exact value (JSON readers need not round-trip doubles)
    add_vec(w, "input/time_step", {c.time_step}, "input: time step (exact)");
  }
  w.parameters()["nb_steps"] = static_cast<long long>(c.nb_steps);
  if (not c.initial_velocity_scale.empty()) {
    auto & v = model.getVelocity();
    for (Idx i = 0; i < v.size(); ++i) {
      for (Int d = 0; d < v.getNbComponent(); ++d) {
        v(i, d) = model.getBlockedDOFs()(i, d) ? 0. : pseudo(i, d, c.initial_velocity_scale[d]);
      }
    }
    add_array(w, "input/velocity", v, "input: initial velocity (v76)");
  }
  const auto ndof = model.getDisplacement().getNbComponent();
  std::vector<double> series, ek, ep;
  std::vector<I64> iterations;
  for (Int s = 1; s <= c.nb_steps; ++s) {
    model.solveStep();
    auto & solver = model.getNonLinearSolver();
    if (c.method != _explicit_lumped_mass) {
      iterations.push_back(Int(solver.get("nb_iterations")));
    }
    if (dynamic) {
      if (c.series_node >= 0) {
        series.push_back(model.getDisplacement()(c.series_node, c.series_dof));
      }
      ek.push_back(model.getEnergy("kinetic"));
      ep.push_back(model.getEnergy("potential"));
      for (auto fs_ : c.full_steps) {
        if (fs_ == s) {
          add_array(w, "series/displacement_step_" + std::to_string(s), model.getDisplacement(),
                    "full displacement after this step");
        }
      }
    }
  }
  record_aij(w, "K", model, "K");
  record_aij(w, "M", model, "M");
  record_state(w, "final", model, dynamic);
  if (not iterations.empty()) {
    add_ivec(w, "series/nb_iterations", iterations, "newton nb_iterations per solveStep");
  }
  if (dynamic) {
    if (not series.empty()) {
      w.parameters()["series/node"] = static_cast<long long>(c.series_node);
      w.parameters()["series/dof"] = static_cast<long long>(c.series_dof);
      add_vec(w, "series/node_displacement", series, "displacement(series_node, series_dof)");
    }
    add_vec(w, "series/kinetic_energy", ek, "getEnergy(\"kinetic\") after each step");
    add_vec(w, "series/potential_energy", ep, "getEnergy(\"potential\") after each step");
  } else {
    add_vec(w, "energy/potential", {model.getEnergy("potential")},
            "getEnergy(\"potential\") after the solve");
    add_vec(w, "energy/kinetic", {model.getEnergy("kinetic")},
            "getEnergy(\"kinetic\") (no mass: 0)");
  }
  (void)ndof;
  w.finish();
}

StructuralMaterial mat(Real E, Real A, Real I, Real Iz, Real Iy, Real GJ, Real rho, Real t = 0.,
                       Real nu = 0.) {
  StructuralMaterial m;
  m.E = E;
  m.A = A;
  m.I = I;
  m.Iz = Iz;
  m.Iy = Iy;
  m.GJ = GJ;
  m.rho = rho;
  m.t = t;
  m.nu = nu;
  return m;
}

std::vector<Real> unit_normal(const std::vector<Real> & a, const std::vector<Real> & b,
                              const std::vector<Real> & helper) {
  Real x[3] = {b[0] - a[0], b[1] - a[1], b[2] - a[2]};
  Real n[3] = {x[1] * helper[2] - x[2] * helper[1], x[2] * helper[0] - x[0] * helper[2],
               x[0] * helper[1] - x[1] * helper[0]};
  Real l = std::sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
  return {n[0] / l, n[1] / l, n[2] / l};
}

/// Line-load tractions per quadrature point (pseudo values), global then
/// local, both added to the external force; recorded.
void line_loads(StructuralMechanicsModel & model, ap::FixtureWriter & w, ElementType type) {
  const auto ne = model.getMesh().getNbElement(type);
  const auto nq = model.getFEEngine().getNbIntegrationPoints(type);
  const auto ndof = model.getDisplacement().getNbComponent();
  Array<Real> global(ne * nq, ndof), local(ne * nq, ndof);
  for (Idx k = 0; k < global.size(); ++k) {
    for (Int d = 0; d < ndof; ++d) {
      global(k, d) = pseudo(k, d, 10. * (d + 1));
      local(k, d) = pseudo(k + 7, d + 3, 5. * (d + 1));
    }
  }
  model.getExternalForce().zero();
  model.computeForcesByGlobalTractionArray(global, type);
  add_array(w, "loads/traction_global", global, "input: global tractions per quadrature point");
  add_array(w, "loads/external_force_global", model.getExternalForce(),
            "external force after computeForcesByGlobalTractionArray");
  model.computeForcesByLocalTractionArray(local, type);
  add_array(w, "loads/traction_local", local, "input: local tractions per quadrature point");
}

std::vector<Case> cases() {
  std::vector<Case> cs;
  const std::string tests = kTests;

  // --- upstream static tests -------------------------------------------------
  {
    Case c;
    c.id = "bernoulli_beam_2_static";
    c.description = "TestStructBernoulli2 (two materials, clamped + rollers, nodal loads), _static";
    c.upstream_test = tests + "test_structural_mechanics_model_bernoulli_beam_2.cc";
    c.dim = 2;
    c.mesh_file = tests + "_bernoulli_beam_2.msh";
    c.materials = {mat(3e10, 0.01, 0.0025, 0, 0, 0, 0), mat(3e10, 0.01, 0.00128, 0, 0, 0, 0)};
    c.element_material = {{_bernoulli_beam_2, {0, 1}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      b(0, 0) = b(0, 1) = b(0, 2) = true;
      b(1, 1) = true;
      b(2, 1) = true;
      const Real M = 3600, q = 6000, L = 10;
      auto & f = m.getExternalForce();
      f(2, 2) = -M;
      f(0, 1) = -q * L / 2;
      f(0, 2) = -q * L * L / 12;
      f(1, 1) = -q * L / 2;
      f(1, 2) = q * L * L / 12;
      f(2, 0) = 3e10 * 0.01 / 18;
    };
    cs.push_back(c);
  }
  {
    Case c;
    c.id = "bernoulli_beam_3_static";
    c.description = "TestStructBernoulli3Static (normals z, nodes 2 and 3 clamped, P = -1 in z), _static";
    c.upstream_test = tests + "test_structural_mechanics_model_bernoulli_beam_3.cc";
    c.dim = 3;
    c.mesh_file = tests + "_bernoulli_beam_3.msh";
    c.normals = {{_bernoulli_beam_3, {{0, 0, 1}, {0, 0, 1}}}};
    c.materials = {mat(1, 1, 0, 1, 1, 1, 0)};
    c.element_material = {{_bernoulli_beam_3, {0, 0}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      for (Idx n : {1, 2}) {
        for (Int d = 0; d < 6; ++d) {
          b(n, d) = true;
        }
      }
      m.getExternalForce().zero();
      m.getExternalForce()(0, 2) = -1;
    };
    cs.push_back(c);
  }
  for (Real t : {1.0, 0.5}) {
    Case c;
    c.id = t == 1.0 ? "discrete_kirchhoff_triangle_18_static"
                    : "discrete_kirchhoff_triangle_18_static_t05";
    c.description = std::string("TestStructDKT18 Batoz patch (E = 1, nu = 0.3, t = ") +
                    (t == 1.0 ? "1" : "0.5") + "), _static";
    c.upstream_test = tests + "test_structural_mechanics_model_discrete_kirchhoff_triangle_18.cc";
    c.dim = 3;
    c.mesh_file = tests + "_discrete_kirchhoff_triangle_18.msh";
    c.materials = {mat(1, 1, 0, 0, 0, 0, 0, t, 0.3)};
    c.element_material = {{_discrete_kirchhoff_triangle_18, {0, 0, 0, 0}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      b.set(true);
      const auto last = b.size() - 1;
      for (Int d = 0; d < 5; ++d) {
        b(last, d) = false;
      }
      auto & u = m.getDisplacement();
      u.zero();
      const Real rows[3][6] = {{40, 20, -800, -20, 40, 0},
                               {50, 40, -1400, -40, 50, 0},
                               {10, 20, -200, -20, 10, 0}};
      for (Idx n = 0; n < 3; ++n) {
        for (Int d = 0; d < 6; ++d) {
          u(n + 1, d) = rows[n][d];
        }
      }
    };
    cs.push_back(c);
  }

  // --- upstream dynamic test ---------------------------------------------------
  for (auto type : {_bernoulli_beam_2, _bernoulli_beam_3}) {
    const Int dim = type == _bernoulli_beam_2 ? 2 : 3;
    const Int ne = 40;
    const Real L = 2, le = L / ne;
    Case c;
    c.id = std::to_string(type).substr(1) + "_dynamic";
    c.description = "TestStructBernoulliDynamic (40 elements, E = 1e9, rho = 10, F = 1e4 on the "
                    "middle node), _implicit_dynamic, dt = 1e-6, 299 steps, newton "
                    "(max_iterations 100, threshold 1e-8, solution criterion)";
    c.upstream_test = tests + "test_structural_mechanics_model_bernoulli_beam_dynamics.cc";
    c.dim = dim;
    for (Int i = 0; i <= ne; ++i) {
      std::vector<Real> x(dim, 0.);
      x[0] = i * le;
      c.nodes.push_back(x);
    }
    std::vector<std::vector<Idx>> conn;
    for (Idx i = 0; i < ne; ++i) {
      conn.push_back({i, i + 1});
    }
    c.blocks = {{type, conn}};
    if (dim == 3) {
      c.normals = {{type, std::vector<std::vector<Real>>(ne, {0., 0., 1.})}};
    }
    c.materials = {mat(1e9, 1, 1, 1, 1, 1, 10)};
    c.element_material = {{type, std::vector<UInt>(ne, 0)}};
    c.method = _implicit_dynamic;
    c.time_step = 1e-6;
    c.nb_steps = 299;
    c.series_node = (ne + 1) / 2;
    c.series_dof = 1;
    c.full_steps = {1, 50, 150, 299};
    c.setup = [dim, ne](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      b.set(false);
      b(0, 0) = b(0, 1) = true;
      b(ne, 1) = true;
      if (dim == 3) {
        b(0, 2) = b(ne, 2) = true;
      }
      m.getExternalForce().zero();
      m.getExternalForce()((ne + 1) / 2, 1) = 1e4;
      auto & solver = m.getNonLinearSolver();
      solver.set("max_iterations", 100);
      solver.set("threshold", 1e-8);
      solver.set("convergence_type", SolveConvergenceCriteria::_solution);
      m.getDisplacement().set(0.);
    };
    cs.push_back(c);
  }

  // --- line loads (UD-293: replayed in legacy mode) ----------------------------
  {
    Case c;
    c.id = "bernoulli_beam_2_line_loads";
    c.description = "_bernoulli_beam_2 inclined chain, global + local traction arrays (forces and "
                    "moments), clamped first node, _static";
    c.dim = 2;
    c.nodes = {{0.1, -0.3}, {1.37, 0.41}, {0.93, 2.2}, {-1.15, 1.7}};
    c.blocks = {{_bernoulli_beam_2, {{0, 1}, {1, 2}, {3, 2}}}};
    c.materials = {mat(2e5, 0.3, 0.02, 0, 0, 0, 7), mat(3e5, 0.4, 0.03, 0, 0, 0, 8)};
    c.element_material = {{_bernoulli_beam_2, {0, 1, 0}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter & w) {
      for (Int d = 0; d < 3; ++d) {
        m.getBlockedDOFs()(0, d) = true;
      }
      line_loads(m, w, _bernoulli_beam_2);
    };
    cs.push_back(c);
  }
  {
    Case c;
    c.id = "bernoulli_beam_3_line_loads";
    c.description = "_bernoulli_beam_3 oriented chain with orthogonal unit normals, global + local "
                    "traction arrays (forces and moments: the theta_y shape row, UD-293), clamped "
                    "first node, _static";
    c.dim = 3;
    c.nodes = {{0.2, -0.1, 0.3}, {1.4, 0.7, -0.5}, {0.6, 2.1, 1.3}, {-1.2, 0.4, 2.2}};
    std::vector<std::vector<Idx>> conn = {{0, 1}, {1, 2}, {2, 3}};
    c.blocks = {{_bernoulli_beam_3, conn}};
    const std::vector<std::vector<Real>> helpers = {{0, 0, 1}, {1, 0, 0}, {0.3, -0.8, 0.5}};
    std::vector<std::vector<Real>> normals;
    for (std::size_t e = 0; e < conn.size(); ++e) {
      normals.push_back(unit_normal(c.nodes[conn[e][0]], c.nodes[conn[e][1]], helpers[e]));
    }
    c.normals = {{_bernoulli_beam_3, normals}};
    c.materials = {mat(2e5, 0.5, 0, 0.02, 0.03, 700, 7.8), mat(3e5, 0.6, 0, 0.04, 0.05, 900, 8.8)};
    c.element_material = {{_bernoulli_beam_3, {0, 1, 1}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter & w) {
      for (Int d = 0; d < 6; ++d) {
        m.getBlockedDOFs()(0, d) = true;
      }
      line_loads(m, w, _bernoulli_beam_3);
    };
    cs.push_back(c);
  }

  // --- explicit consistent mass -------------------------------------------------
  {
    const Int ne = 4;
    Case c;
    c.id = "bernoulli_beam_2_explicit_consistent";
    c.description = "_bernoulli_beam_2 simply supported (4 elements, axial DOFs blocked), "
                    "_explicit_consistent_mass, dt = 1e-3, 50 steps, force on the middle node";
    c.dim = 2;
    for (Int i = 0; i <= ne; ++i) {
      c.nodes.push_back({2.5 * i, 0.});
    }
    c.blocks = {{_bernoulli_beam_2, {{0, 1}, {1, 2}, {2, 3}, {3, 4}}}};
    c.materials = {mat(1., 0.01, 1e-5, 0, 0, 0, 1.)};
    c.element_material = {{_bernoulli_beam_2, {0, 0, 0, 0}}};
    c.method = _explicit_consistent_mass;
    c.time_step = 1e-3;
    c.nb_steps = 50;
    c.series_node = 2;
    c.series_dof = 1;
    c.full_steps = {10, 50};
    c.setup = [ne](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      for (Int n = 0; n <= ne; ++n) {
        b(n, 0) = true;
      }
      b(0, 1) = b(ne, 1) = true;
      m.getExternalForce()(2, 1) = 1e-4;
    };
    cs.push_back(c);
  }

  // --- lumped mass (UD-005) ------------------------------------------------------
  {
    Case c;
    c.id = "bernoulli_beam_2_lumped_mass";
    c.description = "_bernoulli_beam_2 (L = 2, A = 3, rho = 5 and a second beam), "
                    "_explicit_lumped_mass: assembleLumpedMassMatrix and the lumped kinetic energy";
    c.dim = 2;
    c.nodes = {{0, 0}, {2, 0}, {2, 1.5}};
    c.blocks = {{_bernoulli_beam_2, {{0, 1}, {1, 2}}}};
    c.materials = {mat(1, 3, 1, 0, 0, 0, 5)};
    c.element_material = {{_bernoulli_beam_2, {0, 0}}};
    c.method = _explicit_lumped_mass;
    c.lumped_only = true;
    cs.push_back(c);
  }
  {
    Case c;
    c.id = "bernoulli_beam_3_lumped_mass";
    c.description = "_bernoulli_beam_3 (L = 2, A = 3, rho = 5), _explicit_lumped_mass: "
                    "assembleLumpedMassMatrix and the lumped kinetic energy";
    c.dim = 3;
    c.nodes = {{0, 0, 0}, {2, 0, 0}};
    c.blocks = {{_bernoulli_beam_3, {{0, 1}}}};
    c.normals = {{_bernoulli_beam_3, {{0, 0, 1}}}};
    c.materials = {mat(1, 3, 0, 1, 1, 1, 5)};
    c.element_material = {{_bernoulli_beam_3, {0}}};
    c.method = _explicit_lumped_mass;
    c.lumped_only = true;
    cs.push_back(c);
  }

  // --- mixed beam / shell frame ---------------------------------------------------
  {
    Case c;
    c.id = "mixed_beam_shell_static";
    c.description = "3-D frame: two discrete Kirchhoff triangles (a square plate, t = 0.1) with "
                    "_bernoulli_beam_3 edge beams, clamped corner, nodal forces and moments, _static";
    c.dim = 3;
    c.nodes = {{0, 0, 0}, {1, 0, 0}, {1, 1, 0}, {0, 1, 0}};
    c.blocks = {{_bernoulli_beam_3, {{0, 1}, {1, 2}, {2, 3}}},
                {_discrete_kirchhoff_triangle_18, {{0, 1, 2}, {0, 2, 3}}}};
    c.normals = {{_bernoulli_beam_3, {{0, 0, 1}, {0, 0, 1}, {0, 0, 1}}}};
    c.materials = {mat(2e5, 0.01, 0, 2e-5, 1e-5, 30, 7.8), mat(1e5, 1, 0, 0, 0, 0, 7.8, 0.1, 0.25)};
    c.element_material = {{_bernoulli_beam_3, {0, 0, 0}},
                          {_discrete_kirchhoff_triangle_18, {1, 1}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      for (Int d = 0; d < 6; ++d) {
        b(0, d) = true;
        b(3, d) = true;
      }
      auto & f = m.getExternalForce();
      for (Idx n = 1; n < 3; ++n) {
        for (Int d = 0; d < 6; ++d) {
          f(n, d) = pseudo(n, d, d < 3 ? 1. : 0.1);
        }
      }
    };
    cs.push_back(c);
  }
  // ===== adversarial cases ============================================
  // (a) 3-D space frame, beams in general orientation, unit normals orthogonal
  // to each axis built from helper vectors, two materials with Iy != Iz,
  // nodal forces and moments in all six components, global + local line loads
  // (forces and moments, incl. theta_y: UD-293), static.
  {
    Case c;
    c.id = "v76_frame3d_general";
    c.description = "V76: 3-D frame (5 beams, closed loop) in general orientation, non-trivial "
                    "unit extra_normal, Iy != Iz, nodal forces/moments in all 6 DOFs, global + "
                    "local line loads, _static";
    c.dim = 3;
    c.nodes = {{0, 0, 0}, {1.1, 0.4, 0.9}, {2.0, -0.3, 1.7}, {1.3, 1.6, 2.4}, {0.2, 2.2, 1.1}};
    std::vector<std::vector<Idx>> conn = {{0, 1}, {1, 2}, {1, 3}, {3, 4}, {2, 3}};
    c.blocks = {{_bernoulli_beam_3, conn}};
    const std::vector<std::vector<Real>> helpers = {
        {0.2, 0.9, -0.4}, {0.7, -0.1, 0.5}, {-0.3, 0.2, 0.9}, {0.6, 0.6, -0.5}, {0.1, -0.8, 0.3}};
    std::vector<std::vector<Real>> normals;
    for (std::size_t e = 0; e < conn.size(); ++e) {
      normals.push_back(unit_normal(c.nodes[conn[e][0]], c.nodes[conn[e][1]], helpers[e]));
    }
    c.normals = {{_bernoulli_beam_3, normals}};
    c.materials = {mat(2.1e5, 0.012, 0, 3.1e-5, 0.9e-5, 2.2e3, 7.85),
                   mat(0.7e5, 0.02, 0, 0.6e-5, 2.4e-5, 0.8e3, 2.7)};
    c.element_material = {{_bernoulli_beam_3, {0, 1, 0, 1, 1}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter & w) {
      auto & b = m.getBlockedDOFs();
      for (Int d = 0; d < 6; ++d) {
        b(0, d) = true;
      }
      for (Int d = 0; d < 3; ++d) {
        b(4, d) = true;
      }
      line_loads(m, w, _bernoulli_beam_3);
      add_array(w, "loads/external_force_after_local", m.getExternalForce(),
                "external force after the global and local traction arrays (before nodal loads)");
      auto & f = m.getExternalForce();
      for (Idx n = 1; n < 4; ++n) {
        for (Int d = 0; d < 6; ++d) {
          f(n, d) += pseudo(n + 11, d, d < 3 ? 3. : 0.7);
        }
      }
    };
    cs.push_back(c);
  }
  // (b) DKT plate in a tilted plane with an interior node, irregular triangles,
  // every DOF imposed (general displacement and rotation field): stresses and
  // internal forces of the rotated DKT (t != 1: UD-007), static.
  {
    Case c;
    c.id = "v76_dkt_tilted_imposed";
    c.description = "V76: 8 irregular discrete Kirchhoff triangles in a tilted plane (3x3 nodes), "
                    "all DOFs imposed with a general field, t = 0.37, nu = 0.27, _static";
    c.dim = 3;
    // rotation R about axis (1,2,3)/|.| by 0.7 rad, translation (0.3,-0.2,0.5)
    const Real ax[3] = {1. / std::sqrt(14.), 2. / std::sqrt(14.), 3. / std::sqrt(14.)};
    const Real th = 0.7, co = std::cos(th), si = std::sin(th);
    Real R[3][3];
    for (int i = 0; i < 3; ++i) {
      for (int j = 0; j < 3; ++j) {
        R[i][j] = (i == j ? co : 0.) + (1 - co) * ax[i] * ax[j];
      }
    }
    R[0][1] -= si * ax[2]; R[0][2] += si * ax[1];
    R[1][0] += si * ax[2]; R[1][2] -= si * ax[0];
    R[2][0] -= si * ax[1]; R[2][1] += si * ax[0];
    const Real plane[9][2] = {{0, 0}, {0.9, 0.05}, {2.1, -0.1}, {-0.1, 1.0}, {1.15, 0.85},
                              {2.0, 1.1}, {0.05, 2.0}, {0.95, 2.15}, {2.2, 1.9}};
    for (auto & p : plane) {
      std::vector<Real> x(3);
      for (int i = 0; i < 3; ++i) {
        x[i] = R[i][0] * p[0] + R[i][1] * p[1] + (i == 0 ? 0.3 : i == 1 ? -0.2 : 0.5);
      }
      c.nodes.push_back(x);
    }
    c.blocks = {{_discrete_kirchhoff_triangle_18,
                 {{0, 1, 4}, {0, 4, 3}, {1, 2, 5}, {1, 5, 4}, {3, 4, 7}, {3, 7, 6}, {4, 5, 8},
                  {4, 8, 7}}}};
    c.materials = {mat(7e4, 1, 0, 0, 0, 0, 2.7, 0.37, 0.27)};
    c.element_material = {{_discrete_kirchhoff_triangle_18, {0, 0, 0, 0, 0, 0, 0, 0}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      m.getBlockedDOFs().set(true);
      auto & u = m.getDisplacement();
      for (Idx n = 0; n < u.size(); ++n) {
        for (Int d = 0; d < 6; ++d) {
          u(n, d) = pseudo(n + 3, d, d < 3 ? 1e-3 : 2e-3);
        }
      }
    };
    cs.push_back(c);
  }
  // (c) mixed beam / DKT strip in a tilted plane: every node lies on an edge
  // beam (drilling rotations stiffened by in-plane beam bending), clamped end,
  // nodal forces and moments, static.
  {
    Case c;
    c.id = "v76_dkt_beam_tilted";
    c.description = "V76: strip of 6 discrete Kirchhoff triangles (2x4 nodes) in a tilted plane "
                    "with _bernoulli_beam_3 edge beams (normal = plate normal), clamped end, "
                    "nodal forces/moments, _static";
    c.dim = 3;
    const Real e1[3] = {0.48, 0.64, 0.6}; // |e1| = 1
    Real e2[3] = {-0.8, 0.6, 0.};        // orthogonal to e1
    const Real n3[3] = {e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0]};
    const Real plane[8][2] = {{0, 0}, {0.8, -0.05}, {1.7, 0.1}, {2.5, 0.0},
                              {0.05, 0.9}, {0.9, 1.0}, {1.6, 0.95}, {2.45, 1.05}};
    for (auto & p : plane) {
      c.nodes.push_back({p[0] * e1[0] + p[1] * e2[0] + 0.1, p[0] * e1[1] + p[1] * e2[1] - 0.4,
                         p[0] * e1[2] + p[1] * e2[2] + 0.25});
    }
    std::vector<std::vector<Idx>> beams = {{0, 1}, {1, 2}, {2, 3}, {4, 5}, {5, 6}, {6, 7},
                                           {0, 4}, {3, 7}};
    c.blocks = {{_bernoulli_beam_3, beams},
                {_discrete_kirchhoff_triangle_18,
                 {{0, 1, 5}, {0, 5, 4}, {1, 2, 6}, {1, 6, 5}, {2, 3, 7}, {2, 7, 6}}}};
    c.normals = {{_bernoulli_beam_3,
                  std::vector<std::vector<Real>>(beams.size(), {n3[0], n3[1], n3[2]})}};
    c.materials = {mat(2e5, 0.02, 0, 4e-5, 1.5e-5, 40, 7.8), mat(7e4, 1, 0, 0, 0, 0, 2.7, 0.12, 0.3)};
    c.element_material = {{_bernoulli_beam_3, std::vector<UInt>(beams.size(), 0)},
                          {_discrete_kirchhoff_triangle_18, {1, 1, 1, 1, 1, 1}}};
    c.method = _static;
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      for (Int d = 0; d < 6; ++d) {
        b(0, d) = true;
        b(4, d) = true;
      }
      auto & f = m.getExternalForce();
      for (Idx n : {1, 2, 3, 5, 6, 7}) {
        for (Int d = 0; d < 6; ++d) {
          f(n, d) = pseudo(n + 5, d, d < 3 ? 2. : 0.3);
        }
      }
    };
    cs.push_back(c);
  }
  // (d) dynamic 3-D beam in general orientation (polyline), torsion and both
  // bending planes loaded, initial velocity, implicit trapezoidal: consistent
  // mass with the torsion row (UD-507) in every step.
  auto beam3_poly = [](Case & c) {
    c.dim = 3;
    c.nodes = {{0, 0, 0}, {0.4, 0.3, 0.2}, {0.75, 0.7, 0.3}, {1.2, 0.9, 0.55}, {1.5, 1.35, 0.6},
               {1.95, 1.5, 0.95}, {2.3, 1.9, 1.0}};
    std::vector<std::vector<Idx>> conn;
    for (Idx i = 0; i + 1 < Idx(c.nodes.size()); ++i) {
      conn.push_back({i, i + 1});
    }
    c.blocks = {{_bernoulli_beam_3, conn}};
    std::vector<std::vector<Real>> normals;
    for (std::size_t e = 0; e < conn.size(); ++e) {
      const std::vector<Real> h = {std::sin(0.9 * e + 0.2), std::cos(1.7 * e), 0.6 + 0.1 * e};
      normals.push_back(unit_normal(c.nodes[conn[e][0]], c.nodes[conn[e][1]], h));
    }
    c.normals = {{_bernoulli_beam_3, normals}};
    c.materials = {mat(2e5, 0.03, 0, 2.5e-4, 1.1e-4, 60, 7.8)};
    c.element_material = {{_bernoulli_beam_3, std::vector<UInt>(conn.size(), 0)}};
    c.series_node = Idx(c.nodes.size()) - 1;
    c.series_dof = 3;
    c.initial_velocity_scale = {0.05, 0.05, 0.05, 0.4, 0.2, 0.2};
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      for (Int d = 0; d < 6; ++d) {
        b(0, d) = true;
      }
      auto & f = m.getExternalForce();
      const auto last = f.size() - 1;
      f(last, 0) = 0.3;
      f(last, 1) = -0.8;
      f(last, 2) = 0.6;
      f(last, 3) = 0.25; // torsion-dominated moment
      f(last, 4) = -0.05;
      f(last, 5) = 0.07;
      f(3, 3) = -0.1;
    };
  };
  {
    Case c;
    c.id = "v76_beam3_dynamic_general";
    c.description = "V76: 6 _bernoulli_beam_3 on a 3-D polyline, varying normals, clamped, end "
                    "force + torsion/bending moments, initial velocity, _implicit_dynamic, "
                    "dt = 2e-3, 40 steps";
    beam3_poly(c);
    c.method = _implicit_dynamic;
    c.time_step = 2e-3;
    c.nb_steps = 40;
    c.full_steps = {1, 20, 40};
    cs.push_back(c);
  }
  {
    Case c;
    c.id = "v76_beam3_explicit_consistent_general";
    c.description = "V76: the polyline of v76_beam3_dynamic_general, _explicit_consistent_mass, "
                    "dt = 1e-4, 60 steps";
    beam3_poly(c);
    c.method = _explicit_consistent_mass;
    c.time_step = 1e-4;
    c.nb_steps = 60;
    c.full_steps = {1, 30, 60};
    cs.push_back(c);
  }
  // (e) 2-D inclined chain, implicit dynamic (rotated 2-D consistent mass).
  {
    Case c;
    c.id = "v76_beam2_dynamic_inclined";
    c.description = "V76: 4 _bernoulli_beam_2 on an inclined polyline, clamped, end force and "
                    "moment, initial velocity, _implicit_dynamic, dt = 1e-3, 30 steps";
    c.dim = 2;
    c.nodes = {{0, 0}, {0.6, 0.35}, {1.1, 0.9}, {1.75, 1.2}, {2.2, 1.8}};
    c.blocks = {{_bernoulli_beam_2, {{0, 1}, {1, 2}, {2, 3}, {3, 4}}}};
    c.materials = {mat(2e5, 0.02, 3e-4, 0, 0, 0, 7.8)};
    c.element_material = {{_bernoulli_beam_2, {0, 0, 0, 0}}};
    c.method = _implicit_dynamic;
    c.time_step = 1e-3;
    c.nb_steps = 30;
    c.series_node = 4;
    c.series_dof = 2;
    c.full_steps = {1, 30};
    c.initial_velocity_scale = {0.05, 0.05, 0.3};
    c.setup = [](StructuralMechanicsModel & m, ap::FixtureWriter &) {
      auto & b = m.getBlockedDOFs();
      for (Int d = 0; d < 3; ++d) {
        b(0, d) = true;
      }
      m.getExternalForce()(4, 0) = 0.4;
      m.getExternalForce()(4, 1) = -0.9;
      m.getExternalForce()(4, 2) = 0.2;
    };
    cs.push_back(c);
  }
  // (f) UD-510: kinetic energy of an _explicit_lumped_mass analysis before
  // any step (which matrices exist after initFull?), 3-D and 2-D.
  {
    Case c;
    c.id = "v76_beam3_lumped_pre_energy";
    c.description = "V76: polyline of v76_beam3_dynamic_general, _explicit_lumped_mass: matrices "
                    "after initFull and getEnergy(kinetic) before any step";
    beam3_poly(c);
    c.initial_velocity_scale.clear();
    c.setup = nullptr;
    c.method = _explicit_lumped_mass;
    c.pre_energy = true;
    cs.push_back(c);
  }
  {
    Case c;
    c.id = "v76_beam2_lumped_pre_energy";
    c.description = "V76: 2-D inclined chain, _explicit_lumped_mass: matrices after initFull and "
                    "getEnergy(kinetic) before any step";
    c.dim = 2;
    c.nodes = {{0, 0}, {0.6, 0.35}, {1.1, 0.9}, {1.75, 1.2}, {2.2, 1.8}};
    c.blocks = {{_bernoulli_beam_2, {{0, 1}, {1, 2}, {2, 3}, {3, 4}}}};
    c.materials = {mat(2e5, 0.02, 3e-4, 0, 0, 0, 7.8)};
    c.element_material = {{_bernoulli_beam_2, {0, 0, 0, 0}}};
    c.method = _explicit_lumped_mass;
    c.pre_energy = true;
    cs.push_back(c);
  }
  return cs;
}

/* -------------------------------------------------------------------------- */
/* STRUCT-013: setTimeStep and the dumpers */
/* -------------------------------------------------------------------------- */
void record_dump_tree(ap::FixtureWriter & w, const fs::path & dir) {
  std::vector<fs::path> files;
  for (auto & e : fs::recursive_directory_iterator(dir)) {
    if (e.is_regular_file()) {
      files.push_back(e.path());
    }
  }
  std::sort(files.begin(), files.end());
  Json list = Json::array();
  for (auto & f : files) {
    auto rel = fs::relative(f, dir).generic_string();
    list.push(rel);
    w.add_input_file("dump:" + rel, f, "oracle output");
  }
  w.parameters()["dump_files"] = list;
}

/// TestStructBernoulli2's set-up (bernoulli_beam_2_static), model id
/// `dump_set_time_step` (the default dumper's file name), the default
/// dumper with the structural fields and a second ParaView dumper in text
/// mode registered before setTimeStep: not given the time step, so it
/// writes no .pvd. Dumps before and after one static step with every
/// upstream overload.
void dump_set_time_step_case() {
  const std::string id = "dump_set_time_step";
  if (not wanted(id)) {
    return;
  }
  const std::string cid = "model/structural/" + id;
  const auto mesh_rel = std::string(kTests) + "_bernoulli_beam_2.msh";
  Mesh mesh(2);
  mesh.read((g_up / mesh_rel).string(), _miot_gmsh_struct);
  StructuralMechanicsModel model(mesh, _all_dimensions, id);
  auto m0 = mat(3e10, 0.01, 0.0025, 0, 0, 0, 0);
  auto m1 = mat(3e10, 0.01, 0.00128, 0, 0, 0, 0);
  model.addMaterial(m0);
  model.addMaterial(m1);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  {
    auto & em = model.getElementMaterial(_bernoulli_beam_2);
    em(0) = 0;
    em(1) = 1;
    auto & b = model.getBlockedDOFs();
    b(0, 0) = b(0, 1) = b(0, 2) = true;
    b(1, 1) = true;
    b(2, 1) = true;
    const Real M = 3600, q = 6000, L = 10;
    auto & f = model.getExternalForce();
    f(2, 2) = -M;
    f(0, 1) = -q * L / 2;
    f(0, 2) = -q * L * L / 12;
    f(1, 1) = -q * L / 2;
    f(1, 2) = q * L * L / 12;
    f(2, 0) = 3e10 * 0.01 / 18;
  }

  // a scratch directory next to the stage directory (one per run)
  const auto out = g_root.parent_path() / (g_root.filename().string() + "-dump-work") / id;
  fs::remove_all(out);
  fs::create_directories(out); // IOHelper creates only the last level
  const auto pv = (out / "paraview").string();
  model.setBaseName(id);
  mesh.setDirectory(pv);
  model.addDumpFieldVector("displacement");
  model.addDumpField("rotation");
  model.addDumpFieldVector("force");
  model.addDumpField("momentum");
  model.addDumpField("internal_force");
  model.addDumpField("internal_momentum");
  model.addDumpField("blocked_dofs");
  model.addDumpField("stress");
  mesh.registerDumper<DumperParaview>("extra", id + "_extra");
  mesh.addDumpMeshToDumper("extra", mesh, 2, _not_ghost, _ek_structural);
  mesh.setDirectoryToDumper("extra", pv);
  mesh.setTextModeToDumper("extra");
  model.addDumpFieldToDumper("extra", "displacement");
  model.addDumpFieldToDumper("extra", "internal_force");
  const Real dt = 0.125;
  model.setTimeStep(dt);

  ap::FixtureWriter w(g_root, cid,
                      "STRUCT-013 setTimeStep and the dumpers: the default dumper "
                      "structural_mechanics_model and an extra text dumper registered before "
                      "setTimeStep, every file byte for byte",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("structural");
  w.add_input_file("mesh", g_up / mesh_rel, "upstream:" + mesh_rel);
  auto & p = w.parameters();
  p["upstream_test"] = std::string(kTests) + "test_structural_mechanics_model_bernoulli_beam_2.cc";
  p["spatial_dimension"] = 2LL;
  p["analysis_method"] = std::to_string(_static);
  p["sparse_solver"] = "eigen";
  p["time_step"] = dt;

  model.dump();
  model.dump("extra");
  model.solveStep();
  model.dump();
  model.dump("extra");
  model.dump(5);
  model.dump("extra", 7);
  model.dump(0.75, 9);
  model.dump("extra", 1.5, 3);
  record_state(w, "final", model, false);
  record_dump_tree(w, out);
  w.finish();
  fs::remove_all(out.parent_path());
  std::cout << "  " << cid << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 3) {
    std::cerr << "usage: structural_model_probe <fixtures_root> <upstream_src_root>\n";
    return 2;
  }
  // D-050: host-independent blocking of Eigen's dense kernels (SparseLU
  // supernodes, gebp): Eigen's own defaults, the values the port reproduces.
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  g_root = argv[1];
  g_up = argv[2];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    for (auto && c : cases()) {
      run(c);
    }
    dump_set_time_step_case();
  } catch (std::exception & e) {
    std::cerr << "structural_model_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
