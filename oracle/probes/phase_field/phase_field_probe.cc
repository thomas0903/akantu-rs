// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the upstream PhaseFieldModel standalone (strain
// fields written into the laws' `strain` internal, as the solid coupler
// does) with the Eigen sparse solver pinned (bit-reproducible solves,
// D-008/D-035) and records the model state for the Rust differential tests
// (fixtures/model/phase_field/*, crates/akantu-phase-field/tests/
// phase_field_fixtures.rs).
//
// Every case records, per load step: the strain written into each law
// (input of the replay), the nodal damage after solveStep("static"), the
// internal force, the Newton-Raphson iteration count and error, and
// getEnergy("dissipated") (called after the solve, before the optional
// savePreviousState, in this order). After the last step: every internal of
// every law (current and previous values where a history exists), the
// stiffness K (COO, storage order), getEnergy("dissipated", element) of
// every element and getEnergy("dissipated", group) of every element group
// of the model dimension.
//
// Cases (case ids model/phase_field/<case>, format v1):
//   one_element_<file>      the one-element meshes / material files of
//                           test/test_model/test_phase_field_model (uniaxial
//                           strain history of the coupled tests)
//   optimal_quadratic       test_phase_field_optimal.cc (d = 1 on `blocked`)
//   patch_<type>_<law>      upstream patch-test meshes
//                           (test/test_model/patch_tests/data/_<type>.msh),
//                           non-uniform strain fields, damage prescribed on
//                           the nodes of the lowest abscissa, the law
//                           variants (quadratic/linear x no_split/
//                           volumetric_deviatoric x history/penalization/
//                           tao), random gc for some (fixed seed); the
//                           linear + volumetric_deviatoric + history variant
//                           (3 cases added after a verification audit, committed
//) is appended last so the other
//                           variants keep their seeds (12345 + index)
//   multi_law_segments      three laws on a 1-D mesh of unequal segments
//                           (physical-name selector): the upstream behaviour
//                           of UD-377 (unfiltered damage interpolation) and
//                           UD-380 (per-element energy with law-local index)
//   mixed_types_stiffness   two laws on a triangle + quadrangle mesh, the law
//                           of the quadrangle has no triangle: K and f_int
//                           after assembleInternalForces +
//                           assembleStiffnessMatrix (UD-378: upstream stops at
//                           the empty triangle filter), no solve
// rev_* the 9 adversarial cases
// (added): three laws on a mixed
//                           tri+quad mesh, AT1 unloading/recovery with
//                           positive and negative rho_rec, random gc with
//                           another seed, the volumetric-deviatoric split
//                           under compression in 1-D/2-D/3-D, AT1 full
//                           unload without a damage BC (UD-388); they record
//                           `newton/converged` and survive upstream Newton
//                           divergences (record_converged)
//   dump_set_time_step PF-012: the
//                           model's ParaView dumper `phase_field` and an
//                           extra text-mode dumper registered before
//                           setTimeStep; PhaseFieldModel::setTimeStep sets
//                           only `phase_field` (phase_field_model.cc:309-313),
//                           so only its time description (.pvd) is written;
//                           every dumped file recorded byte for byte (inputs
//                           role "dump:<path relative to the dump directory>")
//
// Usage: phase_field_probe <fixtures_root> <upstream_src_root> <work_dir>
// Environment: PF_PROBE_CASES=<id>[,<id>...] restricts the run.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <aka_random_generator.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <non_linear_solver.hh>
#include <phase_field_model.hh>
#include <phasefield.hh>
#include <sparse_matrix_aij.hh>
// dumper headers after mesh.hh (as oracle/probes/dump/dump_probe.cc)
#include <dumpable_inline_impl.hh>
#include <dumper_iohelper_paraview.hh>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <limits>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "phase_field_probe";
constexpr const char * kProbeVersion = "1";

using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up, g_work;

bool wanted(const std::string & id) {
  const char * env = std::getenv("PF_PROBE_CASES");
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

void record_aij(ap::FixtureWriter & w, const std::string & prefix, PhaseFieldModel & model) {
  if (not model.getDOFManager().hasMatrix("K")) {
    return;
  }
  auto & A = aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix("K"));
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
  add_ivec(w, prefix + "/rows", rows, "SparseMatrixAIJ::getIRN - 1 (storage order)");
  add_ivec(w, prefix + "/cols", cols, "SparseMatrixAIJ::getJCN - 1 (storage order)");
  add_vec(w, prefix + "/values", vals, "SparseMatrixAIJ::getA (storage order)");
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

/// strain(step, law, x) -> column-major dim x dim strain at a point.
using StrainFn =
    std::function<void(Int step, Int law, const Vector<Real> & x, Matrix<Real> & eps)>;

struct CaseSpec {
  std::string id;
  std::string description;
  fs::path mesh;          // mesh file
  std::string mesh_role;  // provenance of the mesh
  fs::path material;      // material file
  std::string material_role;
  Int dim{2};
  bool selector{false};   // MeshDataPhaseFieldSelector("physical_names")
  long seed{1};
  Int steps{1};
  bool save{true};        // savePreviousState after each step
  Int max_iterations{100};
  Real threshold{1e-10};
  std::string criterion{"residual"};
  std::string bc_group;   // applyBC(FixedValue(bc_value, _x), group) if set
  std::vector<I64> bc_nodes; // else: damage(n) = bc_value, blocked
  Real bc_value{0.};
  StrainFn strain;
  bool solve{true};
  /// nodal damage written before the first step (`initial_damage`)
  std::function<Real(const Vector<Real> &)> damage_init;
  /// record every law internal (off for large meshes: size policy, 1 MiB)
  bool full_internals{true};
  /// Review cases: catch a failing solveStep (upstream
  /// Newton divergence) instead of aborting the run and record the
  /// `newton/converged` flags; the cases keep `false` (a failure
  /// aborts the probe, their fixtures stay reproducible unchanged)
  bool record_converged{false};
};

std::string write_text(const std::string & name, const std::string & text) {
  auto p = g_work / name;
  std::ofstream f(p);
  f << text;
  return p.string();
}

/// Record every internal of every law (current, and previous for `damage`
/// and `phi`) for the not-ghost types of the model dimension.
void record_internals(ap::FixtureWriter & w, PhaseFieldModel & model, Int dim) {
  static const std::vector<std::string> names{
      "g_c",           "damage",         "grad_d",        "phi",
      "strain",        "driving_force",  "driving_energy", "damage_energy",
      "damage_energy_density", "dissipated_energy"};
  for (Int l = 0; l < model.getNbPhaseFields(); ++l) {
    auto & pf = model.getPhaseField(l);
    // the non-const getElementFilter() overloads are protected upstream
    const auto & cpf = static_cast<const PhaseField &>(pf);
    for (auto type : cpf.getElementFilter().elementTypes(dim, _not_ghost)) {
      auto tname = std::to_string(type);
      const auto & f = cpf.getElementFilter(type, _not_ghost);
      std::vector<I64> fv(f.data(), f.data() + f.size());
      add_ivec(w, "law/" + std::to_string(l) + "/filter/" + tname, fv, "element filter");
      for (const auto & n : names) {
        auto & field = pf.getInternal<Real>(n);
        add_array(w, "law/" + std::to_string(l) + "/" + n + "/" + tname,
                  field(type, _not_ghost), "internal " + n + " (law-local order)");
        if (n == "damage" or n == "phi") {
          add_array(w, "law/" + std::to_string(l) + "/previous_" + n + "/" + tname,
                    field.previous(type, _not_ghost), "history of " + n);
        }
      }
    }
  }
}

void run_case(const CaseSpec & s) {
  if (not wanted(s.id)) {
    return;
  }
  const std::string cid = "model/phase_field/" + s.id;
  getStaticParser().parse(s.material.string());
  RandomGenerator<Idx>::seed(s.seed);
  Mesh mesh(s.dim);
  mesh.read(s.mesh.string());
  PhaseFieldModel model(mesh);
  if (s.selector) {
    model.setPhaseFieldSelector(
        std::make_shared<MeshDataPhaseFieldSelector<std::string>>("physical_names", model));
  }
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver("static");
  solver.set("max_iterations", s.max_iterations);
  solver.set("threshold", s.threshold);
  solver.set("convergence_type", s.criterion == "solution"
                                     ? SolveConvergenceCriteria::_solution
                                     : SolveConvergenceCriteria::_residual);

  ap::FixtureWriter w(g_root, cid, s.description, kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("phase_field");
  w.add_input_file("mesh", s.mesh, s.mesh_role);
  w.add_input_file("material", s.material, s.material_role);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(s.dim);
  p["selector_physical_names"] = s.selector;
  p["seed"] = static_cast<long long>(s.seed);
  p["steps"] = static_cast<long long>(s.steps);
  p["save_previous_state"] = s.save;
  p["newton_max_iterations"] = static_cast<long long>(s.max_iterations);
  p["newton_threshold"] = s.threshold;
  p["newton_convergence_type"] = s.criterion;
  p["sparse_solver_type"] = "eigen";
  p["solve"] = s.solve;
  p["nb_phase_fields"] = static_cast<long long>(model.getNbPhaseFields());
  p["bc_group"] = s.bc_group;
  p["bc_value"] = s.bc_value;
  if (not s.bc_nodes.empty()) {
    add_ivec(w, "bc_nodes", s.bc_nodes, "nodes with prescribed damage (bc_value)");
  }

  auto & damage = model.getDamage();
  auto & blocked = model.getBlockedDOFs();
  if (not s.bc_group.empty()) {
    model.applyBC(BC::Dirichlet::FixedValue(s.bc_value, _x), s.bc_group);
  } else {
    for (auto n : s.bc_nodes) {
      damage(n) = s.bc_value;
      blocked(n) = true;
    }
  }

  if (s.damage_init) {
    const auto & X = mesh.getNodes();
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      Vector<Real> x = make_view(X, mesh.getSpatialDimension()).begin()[n];
      damage(n) = s.damage_init(x);
    }
    add_array(w, "initial_damage", damage, "nodal damage written before the first step");
  }

  auto & fem = model.getFEEngine();
  std::vector<I64> iterations;
  std::vector<double> errors, energies;
  std::vector<I64> converged;
  for (Int k = 0; k < s.steps; ++k) {
    for (Int l = 0; l < model.getNbPhaseFields(); ++l) {
      auto & pf = model.getPhaseField(l);
      const auto & cpf = static_cast<const PhaseField &>(pf);
      auto & strain = pf.getInternal<Real>("strain");
      for (auto type : cpf.getElementFilter().elementTypes(s.dim, _not_ghost)) {
        const auto & filter = cpf.getElementFilter(type, _not_ghost);
        if (filter.empty()) {
          continue;
        }
        Array<Real> coords(0, mesh.getSpatialDimension());
        fem.computeIntegrationPointsCoordinates(coords, type, _not_ghost, filter);
        auto & st = strain(type, _not_ghost);
        for (auto && [x, eps] :
             zip(make_view(coords, mesh.getSpatialDimension()), make_view(st, s.dim, s.dim))) {
          Matrix<Real> e(s.dim, s.dim);
          e.zero();
          Vector<Real> xv = x;
          s.strain(k, l, xv, e);
          eps = e;
        }
        add_array(w, "step/" + std::to_string(k) + "/strain/" + std::to_string(l) + "/" +
                         std::to_string(type),
                  st, "strain written into the law before the step (law-local order)");
      }
    }
    if (s.solve) {
      if (s.record_converged) {
        try {
          model.solveStep("static");
          converged.push_back(1);
        } catch (std::exception & e) {
          converged.push_back(0);
          std::cerr << "  " << cid << " step " << k << ": " << e.what() << std::endl;
        }
      } else {
        model.solveStep("static");
      }
      iterations.push_back(Int(solver.get("nb_iterations")));
      errors.push_back(Real(solver.get("error")));
      add_array(w, "step/" + std::to_string(k) + "/damage", damage, "nodal damage after solveStep");
      add_array(w, "step/" + std::to_string(k) + "/internal_force", model.getInternalForce(),
                "internal force after solveStep");
      energies.push_back(model.getEnergy("dissipated"));
      if (s.save) {
        model.savePreviousState();
      }
    } else {
      model.assembleInternalForces();
      model.assembleStiffnessMatrix();
      add_array(w, "step/" + std::to_string(k) + "/internal_force", model.getInternalForce(),
                "internal force after assembleInternalForces");
      energies.push_back(model.getEnergy("dissipated"));
    }
  }
  if (s.solve) {
    add_ivec(w, "newton/nb_iterations", iterations, "nb_iterations per solveStep");
    if (s.record_converged) {
      add_ivec(w, "newton/converged", converged, "1 if solveStep converged (review probe)");
    }
    add_vec(w, "newton/error", errors, "error per solveStep");
  }
  add_vec(w, "energy/dissipated", energies,
          "getEnergy(\"dissipated\") after each step (solveStep, or the assembly)");
  w.add_bool("blocked_dofs", {static_cast<std::size_t>(blocked.size()), 1},
             {"entry", "component"}, "table",
             std::vector<bool>(blocked.data(), blocked.data() + blocked.size()))
      .description("blocked flags after the boundary conditions");
  w.parameters()["full_internals"] = s.full_internals;
  if (s.full_internals) {
    record_internals(w, model, s.dim);
  }
  record_aij(w, "K", model);
  {
    // per-element and per-group dissipated energies (stored internals)
    for (auto type : mesh.elementTypes(s.dim, _not_ghost)) {
      std::vector<double> ee;
      for (Idx e = 0; e < mesh.getNbElement(type, _not_ghost); ++e) {
        ee.push_back(model.getEnergy("dissipated", Element{type, e, _not_ghost}));
      }
      add_vec(w, "energy/element/" + std::to_string(type), ee,
              "getEnergy(\"dissipated\", element) of every element (mesh order)");
    }
    Json groups = Json::array();
    std::vector<double> ge;
    for (auto & g : mesh.iterateElementGroups()) {
      if (g.getDimension() != s.dim) {
        continue;
      }
      groups.push(g.getName());
      ge.push_back(model.getEnergy("dissipated", g.getName()));
    }
    p["energy_groups"] = groups;
    if (not ge.empty()) {
      add_vec(w, "energy/group", ge,
              "getEnergy(\"dissipated\", group) (groups of energy_groups)");
    }
  }
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* cases                                                                      */
/* -------------------------------------------------------------------------- */
fs::path pf_data() { return g_up / "test/test_model/test_phase_field_model"; }
std::string pf_role(const std::string & f) {
  return "upstream:test/test_model/test_phase_field_model/" + f;
}

/// Uniaxial eps_yy = value (eps_xx in 1-D).
void uniaxial(Real value, Matrix<Real> & eps) {
  auto k = eps.rows() == 1 ? 0 : 1;
  eps(k, k) = value;
}

void one_element_cases() {
  struct OneElement {
    std::string id, msh, dat;
    Int steps;
    bool save;
    Int max_it;
    Real threshold;
    std::string criterion;
    std::function<Real(Int)> history;
  };
  auto ramp = [](Real inc) { return [inc](Int k) { return inc * Real(k); }; };
  auto cycle = [](Real inc, Int n) {
    return [inc, n](Int k) {
      if (k < n) {
        return inc * Real(k);
      }
      if (k < 2 * n) {
        return (3. * Real(n) - 2. * Real(k)) * inc;
      }
      return (3. * Real(k) - 7. * Real(n)) * inc;
    };
  };
  std::vector<OneElement> cases{
      {"one_element_coupling", "test_one_element.msh", "material_coupling.dat", 12, false, 100,
       1e-10, "residual", ramp(2e-3)},
      {"one_element_deviatoric", "test_one_element.msh", "material_deviatoric.dat", 18, true,
       1000, 1e-6, "solution", cycle(5e-4, 6)},
      {"one_element_penalization", "test_one_element.msh", "material_penalization.dat", 18,
       true, 1000, 1e-6, "residual", cycle(2e-3, 6)},
      {"one_element_linear", "test_one_element.msh", "material_linear.dat", 12, false, 100,
       1e-10, "residual", ramp(2e-3)},
      {"four_elements_penalization", "test_four_elements.msh", "material_penalization.dat", 16,
       true, 1000, 1e-6, "solution", cycle(1e-3, 6)},
  };
  for (auto & c : cases) {
    CaseSpec s;
    s.id = c.id;
    s.description = "phase field alone on " + c.msh + " with " + c.dat +
                    " (uniaxial strain history of the coupled upstream tests)";
    s.mesh = pf_data() / c.msh;
    s.mesh_role = pf_role(c.msh);
    s.material = pf_data() / c.dat;
    s.material_role = pf_role(c.dat);
    s.dim = 2;
    s.selector = true;
    s.steps = c.steps;
    s.save = c.save;
    s.max_iterations = c.max_it;
    s.threshold = c.threshold;
    s.criterion = c.criterion;
    auto h = c.history;
    s.strain = [h](Int k, Int, const Vector<Real> &, Matrix<Real> & eps) { uniaxial(h(k), eps); };
    run_case(s);
  }
  {
    CaseSpec s;
    s.id = "optimal_quadratic";
    s.description = "test_phase_field_optimal.cc: AT2, no strain, d = 1 on group blocked";
    s.mesh = pf_data() / "test_optimal_damage.msh";
    s.mesh_role = pf_role("test_optimal_damage.msh");
    s.material = pf_data() / "material_optimal.dat";
    s.material_role = pf_role("material_optimal.dat");
    s.dim = 2;
    s.steps = 1;
    s.max_iterations = 1000;
    s.threshold = 1e-6;
    s.criterion = "residual";
    s.bc_group = "blocked";
    s.bc_value = 1.;
    s.full_internals = false; // 999 elements x 4 points: above 1 MiB
    s.strain = [](Int, Int, const Vector<Real> &, Matrix<Real> &) {};
    run_case(s);
  }
}

/// Probe-defined law variants of the patch cases.
struct LawVariant {
  std::string tag, law, split, irreversibility, gc;
};

std::string material_text(const LawVariant & v, Int dim) {
  std::stringstream ss;
  ss << "phasefield " << v.law << (v.split.empty() ? "" : " " + v.split) << " [\n"
     << "  name = body\n  E = 210.0\n  nu = 0.3\n  gc = " << v.gc << "\n  l0 = 0.2\n"
     << "  Plane_Stress = " << (dim == 2 ? "true" : "false") << "\n"
     << "  irreversibility_type = " << v.irreversibility << "\n"
     << "  irreversibility_tol = 1e-3\n";
  if (v.law == "linear") {
    ss << "  recovery_tol = 1e-3\n";
  }
  ss << "]\n";
  return ss.str();
}

void patch_cases() {
  const auto data = g_up / "test/test_model/patch_tests/data";
  const std::vector<LawVariant> variants{
      {"quadratic_history", "quadratic", "no_split", "history", "5e-3"},
      {"quadratic_voldev_penalization", "quadratic", "volumetric_deviatoric", "penalization",
       "5e-3"},
      {"linear_history", "linear", "no_split", "history", "5e-3"},
      {"linear_voldev_penalization", "linear", "volumetric_deviatoric", "penalization", "5e-3"},
      {"quadratic_tao_random_gc", "quadratic", "", "tao", "5e-3 uniform [0, 1e-3]"},
      // added after a verification audit: AT1 + history + vol/dev
      {"linear_voldev_history", "linear", "volumetric_deviatoric", "history", "5e-3"},
  };
  struct PatchType {
    ElementType type;
    std::vector<std::size_t> variants;
  };
  const std::vector<PatchType> types{
      {_segment_2, {0, 2, 5}},          {_segment_3, {1}},     {_triangle_3, {0, 1, 2, 3, 4, 5}},
      {_triangle_6, {1, 2}},         {_quadrangle_4, {0, 3, 4}}, {_quadrangle_8, {2}},
      {_tetrahedron_4, {0, 1, 3}},   {_tetrahedron_10, {2}}, {_hexahedron_8, {1, 4, 5}},
      {_pentahedron_6, {0}},         {_hexahedron_20, {3}},
  };
  for (const auto & t : types) {
    const auto dim = Mesh::getSpatialDimension(t.type);
    const auto tname = std::to_string(t.type);
    for (auto vi : t.variants) {
      const auto & v = variants[vi];
      CaseSpec s;
      s.id = "patch" + tname + "_" + v.tag;
      s.description = "phase field on the patch-test mesh " + tname + ", " + v.law + " " +
                      v.split + ", irreversibility " + v.irreversibility +
                      ", non-uniform strain history, damage prescribed at x = min";
      s.mesh = data / (tname + ".msh");
      s.mesh_role = "upstream:test/test_model/patch_tests/data/" + tname + ".msh";
      s.material = write_text(s.id + ".dat", material_text(v, dim));
      s.material_role = "probe:material " + s.id;
      s.dim = dim;
      s.seed = 12345 + Int(vi);
      s.steps = 6;
      s.save = true;
      // solution criterion: the residual of these cases stalls at its
      // rounding floor (~1e-13) above a residual threshold of 1e-12
      s.max_iterations = 50;
      s.threshold = 1e-10;
      s.criterion = "solution";
      // damage 0 on the nodes of the lowest abscissa
      Mesh probe(dim);
      probe.read(s.mesh.string());
      Real xmin = std::numeric_limits<Real>::max();
      for (Idx n = 0; n < probe.getNbNodes(); ++n) {
        xmin = std::min(xmin, probe.getNodes()(n, 0));
      }
      for (Idx n = 0; n < probe.getNbNodes(); ++n) {
        if (probe.getNodes()(n, 0) == xmin) {
          s.bc_nodes.push_back(n);
        }
      }
      s.bc_value = 0.;
      // eps_ij(x) = t * (a_ij + b_ij * x_0 + c_ij * x_last), symmetric,
      // t = 0.004 * (k + 1) for k < 4, then unloading to compression
      s.strain = [dim](Int k, Int, const Vector<Real> & x, Matrix<Real> & eps) {
        const Real t = k < 4 ? 4e-3 * Real(k + 1) : 4e-3 * Real(6 - 2 * k);
        for (Int i = 0; i < dim; ++i) {
          for (Int j = i; j < dim; ++j) {
            const Real a = (i == j) ? 1.0 - 0.3 * Real(i) : 0.2 * Real(i + j);
            const Real b = 0.5 / Real(1 + i + j);
            const Real c = (i + j) % 2 == 0 ? -0.25 : 0.4;
            eps(i, j) = t * (a + b * x(0) + c * x(x.size() - 1));
            eps(j, i) = eps(i, j);
          }
        }
      };
      run_case(s);
    }
  }
}

void multi_law_case() {
  CaseSpec s;
  s.id = "multi_law_segments";
  s.description = "three quadratic phase fields on unequal segments (physical-name selector), "
                  "prescribed damage d = x / 4.5, no solve: UD-377 (unfiltered damage "
                  "interpolation) and UD-380 (per-element energy with the law-local index) in "
                  "the upstream behaviour";
  s.mesh = write_text("multi_law_segments.msh",
                      "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n"
                      "$PhysicalNames\n3\n1 1 \"chocolate\"\n1 2 \"chewing-gum\"\n1 3 "
                      "\"candy\"\n$EndPhysicalNames\n"
                      "$Nodes\n4\n1 0 0 0\n2 1 0 0\n3 2.5 0 0\n4 4.5 0 0\n$EndNodes\n"
                      "$Elements\n3\n1 1 2 1 5 1 2\n2 1 2 2 6 2 3\n3 1 2 3 7 3 4\n$EndElements\n");
  s.mesh_role = "probe:1-D mesh, segments of length 1, 1.5, 2";
  s.material = write_text(
      "multi_law_segments.dat",
      "phasefield quadratic no_split [\n  name = chocolate\n  E = 2\n  gc = 0.02\n  l0 = 0.5\n]\n"
      "phasefield quadratic no_split [\n  name = chewing-gum\n  E = 1\n  gc = 0.01\n  l0 = "
      "0.7\n]\n"
      "phasefield quadratic no_split [\n  name = candy\n  E = 3\n  gc = 0.03\n  l0 = 0.9\n]\n");
  s.material_role = "probe:material multi_law_segments";
  // no solve: with UD-377 the residual uses another element's damage, so K
  // is not its Jacobian and Newton-Raphson does not converge upstream
  // (observed: error 2.4e-3 after 50 iterations); the case records the
  // residual, K, internals and energies of a prescribed damage profile
  s.dim = 1;
  s.selector = true;
  s.steps = 1;
  s.save = false;
  s.solve = false;
  s.damage_init = [](const Vector<Real> & x) { return x(0) / 4.5; };
  s.strain = [](Int k, Int law, const Vector<Real> & x, Matrix<Real> & eps) {
    eps(0, 0) = 0.05 * Real(k + 1) * (1. + 0.3 * Real(law) + 0.1 * x(0));
  };
  run_case(s);
}

void mixed_types_case() {
  CaseSpec s;
  s.id = "mixed_types_stiffness";
  s.description = "two quadratic phase fields on two triangles (law tri) and one quadrangle "
                  "(law quad): K and f_int after assembleInternalForces + "
                  "assembleStiffnessMatrix (UD-378: upstream stops at the empty triangle "
                  "filter of law quad), no solve";
  s.mesh = write_text("mixed_types.msh",
                      "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n"
                      "$PhysicalNames\n2\n2 1 \"tri\"\n2 2 \"quad\"\n$EndPhysicalNames\n"
                      "$Nodes\n6\n1 0 0 0\n2 1 0 0\n3 1 1 0\n4 0 1 0\n5 2.5 0 0\n6 2 1.2 "
                      "0\n$EndNodes\n"
                      "$Elements\n3\n1 2 2 1 1 1 2 3\n2 2 2 1 1 1 3 4\n3 3 2 2 2 2 5 6 "
                      "3\n$EndElements\n");
  s.mesh_role = "probe:2 triangles + 1 quadrangle";
  s.material = write_text(
      "mixed_types.dat",
      "phasefield quadratic no_split [\n  name = tri\n  E = 210\n  nu = 0.3\n  gc = 5e-3\n  l0 = "
      "0.3\n]\n"
      "phasefield quadratic no_split [\n  name = quad\n  E = 100\n  nu = 0.2\n  gc = 4e-3\n  l0 "
      "= 0.4\n]\n");
  s.material_role = "probe:material mixed_types";
  s.dim = 2;
  s.selector = true;
  s.steps = 1;
  s.solve = false;
  s.strain = [](Int, Int law, const Vector<Real> & x, Matrix<Real> & eps) {
    eps(0, 0) = 1e-3 * (1. + x(0));
    eps(1, 1) = 2e-3 * (1. + 0.5 * Real(law) + x(1));
    eps(0, 1) = eps(1, 0) = 5e-4;
  };
  run_case(s);
}

/* -------------------------------------------------------------------------- */
/* adversarial cases (fixtures rev_*) */
/* -------------------------------------------------------------------------- */
void patch_strain(Int dim, Real t, const Vector<Real> & x, Matrix<Real> & eps, Real shift) {
  for (Int i = 0; i < dim; ++i) {
    for (Int j = i; j < dim; ++j) {
      const Real a = (i == j) ? 1.0 - 0.3 * Real(i) + shift : 0.2 * Real(i + j);
      const Real b = 0.5 / Real(1 + i + j);
      const Real c = (i + j) % 2 == 0 ? -0.25 : 0.4;
      eps(i, j) = t * (a + b * x(0) + c * x(x.size() - 1));
      eps(j, i) = eps(i, j);
    }
  }
}

std::vector<I64> xmin_nodes(const fs::path & mesh, Int dim) {
  Mesh probe(dim);
  probe.read(mesh.string());
  Real xmin = std::numeric_limits<Real>::max();
  for (Idx n = 0; n < probe.getNbNodes(); ++n) {
    xmin = std::min(xmin, probe.getNodes()(n, 0));
  }
  std::vector<I64> out;
  for (Idx n = 0; n < probe.getNbNodes(); ++n) {
    if (probe.getNodes()(n, 0) == xmin) {
      out.push_back(n);
    }
  }
  return out;
}

/* -------------------------------------------------------------------------- */
/* PF-012: setTimeStep and the dumpers */
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

void dump_set_time_step_case() {
  const std::string id = "dump_set_time_step";
  if (not wanted(id)) {
    return;
  }
  const std::string cid = "model/phase_field/" + id;
  const auto mesh_path = g_up / "test/test_model/patch_tests/data/_quadrangle_4.msh";
  const auto mat = write_text(
      id + ".dat",
      "phasefield quadratic [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.2\n]\n");
  getStaticParser().parse(mat);
  Mesh mesh(2);
  mesh.read(mesh_path.string());
  PhaseFieldModel model(mesh);
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());

  const auto out = g_work / id;
  fs::remove_all(out);
  fs::create_directories(out); // IOHelper creates only the last level
  const auto pv = (out / "paraview").string();
  model.setBaseName(id);
  mesh.setDirectory(pv);
  for (std::string f :
       {"damage", "external_force", "internal_force", "blocked_dofs", "partitions"}) {
    model.addDumpField(f);
  }
  // a second ParaView dumper registered before setTimeStep (text mode): not
  // given the time step, so it writes no .pvd
  mesh.registerDumper<DumperParaview>("extra", id + "_extra");
  mesh.addDumpMeshToDumper("extra", mesh, 2, _not_ghost, _ek_regular);
  mesh.setDirectoryToDumper("extra", pv);
  mesh.setTextModeToDumper("extra");
  model.addDumpFieldToDumper("extra", "damage");
  model.addDumpFieldToDumper("extra", "internal_force");
  const Real dt = 0.125;
  model.setTimeStep(dt);

  ap::FixtureWriter w(g_root, cid,
                      "PF-012 setTimeStep and the dumpers: the model dumper phase_field and an "
                      "extra text dumper registered before setTimeStep, every file byte for byte",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("phase_field");
  w.add_input_file("mesh", mesh_path, "upstream:test/test_model/patch_tests/data/_quadrangle_4.msh");
  w.add_input_file("material", mat, "probe:material " + id);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["sparse_solver_type"] = "eigen";
  p["time_step"] = dt;
  p["strain_yy"] = 3e-3;

  model.dump();
  model.dump("extra");
  // uniform uniaxial strain eps_yy = 3e-3 in the law, one static step
  for (Int l = 0; l < model.getNbPhaseFields(); ++l) {
    auto & pf = model.getPhaseField(l);
    const auto & cpf = static_cast<const PhaseField &>(pf);
    auto & strain = pf.getInternal<Real>("strain");
    for (auto type : cpf.getElementFilter().elementTypes(2, _not_ghost)) {
      for (auto && eps : make_view(strain(type, _not_ghost), 2, 2)) {
        eps.zero();
        eps(1, 1) = 3e-3;
      }
    }
  }
  model.solveStep("static");
  model.dump();
  model.dump("extra");
  model.dump(5);
  model.dump("extra", 7);
  model.dump(0.75, 9);
  model.dump("extra", 1.5, 3);
  add_array(w, "damage", model.getDamage(), "nodal damage after solveStep");
  add_array(w, "internal_force", model.getInternalForce(), "internal force after solveStep");
  record_dump_tree(w, out);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

void review_cases() {
  const auto data = g_up / "test/test_model/patch_tests/data";
  // ADV-1: three laws (AT2 vol-dev penalization, AT1 no-split history,
  // AT2 tao) on a mixed triangle + quadrangle plane-strain mesh; law c has
  // no triangle (UD-378), laws a/b do not cover whole types (UD-377/380);
  // prescribed damage with negative values, compression in law a; no solve
  {
    CaseSpec s;
    s.record_converged = true;
    s.id = "rev_mixed_three_laws";
    s.description = "V67 review: AT2 voldev penalization + AT1 history + AT2 tao on tri+quad";
    s.mesh = write_text("rev_mixed.msh",
        "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n"
        "$PhysicalNames\n3\n2 1 \"a\"\n2 2 \"b\"\n2 3 \"c\"\n$EndPhysicalNames\n"
        "$Nodes\n10\n1 0 0 0\n2 1 0 0\n3 2 0 0\n4 3 0 0\n5 0 1 0\n6 1 1.1 0\n7 2 0.9 0\n"
        "8 3 1 0\n9 0.5 2 0\n10 2.5 2 0\n$EndNodes\n"
        "$Elements\n6\n"
        "1 2 2 1 1 1 2 6\n2 2 2 1 1 1 6 5\n3 2 2 2 2 5 6 9\n"
        "4 3 2 2 2 2 3 7 6\n5 3 2 1 1 3 4 8 7\n6 3 2 3 3 6 7 10 9\n$EndElements\n");
    s.mesh_role = "probe:V67 mixed tri+quad, 3 physical names";
    s.material = write_text("rev_mixed.dat",
        "phasefield quadratic volumetric_deviatoric [\n name = a\n E = 210\n nu = 0.3\n gc = 5e-3\n"
        " l0 = 0.3\n irreversibility_type = penalization\n irreversibility_tol = 1e-2\n]\n"
        "phasefield linear no_split [\n name = b\n E = 150\n nu = 0.25\n gc = 8e-3\n l0 = 0.45\n]\n"
        "phasefield quadratic no_split [\n name = c\n E = 90\n nu = 0.2\n gc = 3e-3\n l0 = 0.25\n"
        " irreversibility_type = tao\n]\n");
    s.material_role = "probe:V67 rev_mixed";
    s.dim = 2;
    s.selector = true;
    s.steps = 2;
    s.save = false;
    s.solve = false;
    s.damage_init = [](const Vector<Real> & x) { return 0.3 - 0.25 * x(0) + 0.2 * x(1); };
    s.strain = [](Int k, Int law, const Vector<Real> & x, Matrix<Real> & eps) {
      const Real t = 1e-3 * Real(k + 1);
      eps(0, 0) = t * (law == 0 ? -3. + x(0) : 1. + 0.5 * x(0));
      eps(1, 1) = t * (law == 0 ? -1. - x(1) : 2. + x(1) * Real(law));
      eps(0, 1) = eps(1, 0) = t * 0.7 * (1. + x(1));
    };
    run_case(s);
  }
  // ADV-2: AT1 vol-dev penalization, loading / unloading into compression /
  // reloading, small l0 (rho_rec > 0)
  for (auto && [id, l0] : std::vector<std::pair<std::string, std::string>>{
           {"rev_at1_unload_recovery", "0.05"}, {"rev_at1_negative_rho_rec", "1.0"}}) {
    CaseSpec s;
    s.record_converged = true;
    s.id = id;
    s.description = "V67 review: AT1 voldev penalization, unloading/recovery, l0 = " + l0;
    s.mesh = data / "_quadrangle_4.msh";
    s.mesh_role = "upstream:test/test_model/patch_tests/data/_quadrangle_4.msh";
    s.material = write_text(id + ".dat",
        "phasefield linear volumetric_deviatoric [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n"
        " l0 = " + l0 + "\n Plane_Stress = false\n irreversibility_type = penalization\n"
        " irreversibility_tol = 1e-2\n recovery_tol = 1e-2\n]\n");
    s.material_role = "probe:V67 " + id;
    s.dim = 2;
    s.steps = 8;
    s.save = true;
    s.max_iterations = 50;
    s.threshold = 1e-10;
    s.criterion = "solution";
    s.bc_nodes = xmin_nodes(s.mesh, 2);
    s.bc_value = 0.;
    s.strain = [](Int k, Int, const Vector<Real> & x, Matrix<Real> & eps) {
      static const Real ts[8] = {4e-3, 8e-3, 1.2e-2, -6e-3, -2e-3, 1e-3, 8e-3, 1.6e-2};
      patch_strain(2, ts[k], x, eps, 0.);
    };
    run_case(s);
  }
  // ADV-3: random gc, other seed, other distribution bounds, 9 points
  {
    CaseSpec s;
    s.record_converged = true;
    s.id = "rev_random_gc_seed_quadrangle_8";
    s.description = "V67 review: AT1 history, gc = 4e-3 uniform [0, 2e-3], seed 987654";
    s.mesh = data / "_quadrangle_8.msh";
    s.mesh_role = "upstream:test/test_model/patch_tests/data/_quadrangle_8.msh";
    s.material = write_text("rev_random.dat",
        "phasefield linear [\n name = body\n E = 210\n nu = 0.3\n gc = 4e-3 uniform [0, 2e-3]\n"
        " l0 = 0.2\n Plane_Stress = true\n]\n");
    s.material_role = "probe:V67 rev_random";
    s.dim = 2;
    s.seed = 987654;
    s.steps = 4;
    s.save = true;
    s.max_iterations = 50;
    s.threshold = 1e-10;
    s.criterion = "solution";
    s.bc_nodes = xmin_nodes(s.mesh, 2);
    s.strain = [](Int k, Int, const Vector<Real> & x, Matrix<Real> & eps) {
      static const Real ts[4] = {5e-3, 1e-2, 2e-3, 1.4e-2};
      patch_strain(2, ts[k], x, eps, 0.1);
    };
    run_case(s);
  }
  // ADV-4: volumetric-deviatoric under compression (3-D hexahedron, 2-D
  // plane strain triangle, 1-D segment), history
  struct VD {
    std::string id;
    std::string msh;
    Int dim;
    std::string extra;
  };
  for (auto && v : std::vector<VD>{{"rev_voldev_compression_hexahedron_8", "_hexahedron_8.msh", 3, ""},
                                   {"rev_voldev_compression_triangle_3_plane_strain", "_triangle_3.msh", 2,
                                    " Plane_Stress = false\n"},
                                   {"rev_voldev_compression_segment_2", "_segment_2.msh", 1, ""}}) {
    CaseSpec s;
    s.record_converged = true;
    s.id = v.id;
    s.description = "V67 review: AT2 voldev history under compression then tension";
    s.mesh = data / v.msh;
    s.mesh_role = "upstream:test/test_model/patch_tests/data/" + v.msh;
    s.material = write_text(v.id + ".dat",
        "phasefield quadratic volumetric_deviatoric [\n name = body\n E = 100\n nu = 0.25\n"
        " gc = 1e-3\n l0 = 0.15\n" + v.extra + "]\n");
    s.material_role = "probe:V67 " + v.id;
    s.dim = v.dim;
    s.steps = 5;
    s.save = true;
    s.max_iterations = 50;
    s.threshold = 1e-10;
    s.criterion = "solution";
    s.bc_nodes = xmin_nodes(s.mesh, v.dim);
    const Int dim = v.dim;
    s.strain = [dim](Int k, Int, const Vector<Real> & x, Matrix<Real> & eps) {
      static const Real ts[5] = {-4e-3, -1.2e-2, -2e-2, 6e-3, 1.5e-2};
      patch_strain(dim, ts[k], x, eps, 0.4);
    };
    run_case(s);
  }

  // ADV-5: AT1 penalization, uniform load then full unload, no damage BC
  // (singular tangent at d == d_prev with phi = 0)
  for (auto && msh : std::vector<std::string>{"_triangle_3.msh", "_quadrangle_4.msh"}) {
    CaseSpec s;
    s.record_converged = true;
    s.id = "rev_at1_full_unload" + msh.substr(0, msh.size() - 4);
    s.description = "V67 review: AT1 penalization, uniform load then zero strain, no BC";
    s.mesh = data / msh;
    s.mesh_role = "upstream:test/test_model/patch_tests/data/" + msh;
    s.material = write_text(s.id + ".dat",
        "phasefield linear [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.05\n"
        " irreversibility_type = penalization\n irreversibility_tol = 2e-2\n recovery_tol = 2e-2\n]\n");
    s.material_role = "probe:V67 " + s.id;
    s.dim = 2;
    s.steps = 2;
    s.save = true;
    s.max_iterations = 100;
    s.threshold = 1e-13;
    s.criterion = "solution";
    s.strain = [](Int k, Int, const Vector<Real> &, Matrix<Real> & eps) {
      eps(1, 1) = k == 0 ? 2e-2 : 0.;
    };
    run_case(s);
  }
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: " << argv[0] << " <fixtures_root> <upstream_src_root> <work_dir>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  g_work = argv[3];
  fs::create_directories(g_work);
  int fake_argc = 1;
  char * fake_argv[] = {argv[0], nullptr};
  char ** fake = fake_argv;
  initialize(fake_argc, fake);
  try {
    one_element_cases();
    patch_cases();
    multi_law_case();
    mixed_types_case();
    review_cases();
    dump_set_time_step_case();
  } catch (std::exception & e) {
    std::cerr << "phase_field_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
