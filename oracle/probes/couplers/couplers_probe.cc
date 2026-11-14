// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the upstream model couplers
// (src/model/model_couplers: CouplerSolidPhaseField, CouplerSolidContact,
// CouplerSolidCohesiveContact, CohesiveContactSolverCallback) and records
// their trajectories for the Rust differential tests
// (crates/akantu-couplers/tests/coupler_fixtures.rs).
//
// Every static solve pins the Eigen sparse solver (D-035); Eigen's cache
// sizes are pinned to its defaults at the start of main (D-050).
//
// Cases (case ids model/couplers/<case>, fixture format v1):
//   pf_coupling_static      test_phase_solid_coupling.cc setup through
//                           coupler.solve("static", "static"), 1000 steps
//   pf_coupling_explicit    test_phase_solid_explicit.cc (explicit lumped
//                           solid, 0.8 x stable step), 1000 steps
//   pf_deviatoric           test_phase_field_deviatoric_split.cc, 1500 steps,
//                           savePreviousState after each step
//   pf_anisotropic          test_phase_field_anisotropic.cc (penalization),
//                           1500 steps, savePreviousState
//   pf_penalization         test_penalization.cc (four elements), 1000 steps
//   pf_linear               test_phase_field_linear.cc setup through the
//                           coupler (AT1), 1000 steps
//   pf_multi_material       test_multi_material.cc (two materials / phase
//                           fields, explicit solid, default solvers): the
//                           failure of the phase-field Newton is recorded
//   pf_multi_material_converging  the same with the phase-field Newton at
//                           1000 iterations / 1e-10 (solution), 100 steps:
//                           upstream's unfiltered damage transfer (UD-382)
//                           and interpolation (UD-377) on a converging run
//   pf_linear_penalization, pf_deviatoric_penalization, pf_linear_deviatoric,
//   pf_linear_deviatoric_penalization
//                           (version 3, the cells of the
//                           acceptance table O4 split x AT1 / AT2 x
//                           history / penalization that the cases above do
//                           not run through the coupler: no_split AT1
//                           penalization, vol/dev AT2 penalization, vol/dev
//                           AT1 history, vol/dev AT1 penalization) with a
//                           probe-written material file (the solid
//                           `material phasefield <split>` and the law
//                           `phasefield <law> <split>` of material_coupling.dat
//                           with l0 = 0.3, irreversibility_tol = recovery_tol
//                           = 1e-3) on test_four_elements.msh (the middle
//                           row is free in y: the damaged stiffness feeds
//                           back into the strain), load / unload into
//                           compression / reload in steps of 2e-5 that never
//                           hit a zero strain (an AT1 penalization step at
//                           zero strain has a singular upstream tangent,
//                           UD-388), 1500 steps, savePreviousState
//   contact_static_hertz    examples/.../hertz/contact_explicit_static.cc
//                           (coupler.initFull(_static), Newton + contact
//                           search in the corrector), 100 steps
//   cohesive_contact_explicit  examples/.../hertz/
//                           cohesive_contact_explicit_dynamic.cc with a
//                           contact penalty epsilon_n = 1e3 (the example's 0
//                           disables the contact), 2500 loading + 5000
//                           unloading steps of 4e-7
//   cohesive_contact_callback  the same setup solved with
//                           solid.solveStep(CohesiveContactSolverCallback),
//                           3000 steps
//   cohesive_static the
//                           cohesive-contact example mesh through
//                           CouplerSolidCohesiveContact with _static,
//                           intrinsic cohesive_linear (zero strength,
//                           UD-224), epsilon_n = 1e3, compression of
//                           1e-3/50 per step until the first Newton failure
//                           (at most 10 steps, 100 iterations): every step's
//                           state, a per-callback-event trace of steps 0 and
//                           1 (K, J, residual, solution, detector positions,
//                           gaps, normals, contact elements) and the
//                           composition of K (UD-772: the contact stiffness
//                           goes into the contact model's own DOF manager)
//   cohesive_implicit_dynamic_qs, cohesive_implicit_dynamic_dt0p01
//                           (version 2) the cohesive_static
//                           set-up with _implicit_dynamic and the time step
//                           1e6 (quasi-static) or 1e-2, until the first
//                           Newton failure (at most 6 steps): per step the
//                           state (displacement, velocity, acceleration,
//                           forces, gaps, contact elements) and the Newton
//                           outcome; per step a per-iterate trace (every
//                           residual, and at every corrector the solution
//                           and the displacement): a failed solve restores
//                           the DOFs, so the iterates are the comparison
//   friction_cohesive_contact_explicit (version 4;
//                           **UD-001 variant only**) the
//                           cohesive_contact_explicit set-up with Coulomb
//                           friction on the new crack faces (mu = 0.3,
//                           epsilon_n = epsilon_t = 1e3): the bottom
//                           (`fixed`) blocked in x instead of the sides, the
//                           example's 2500 loading + 5000 unloading steps of
//                           4e-7 in y and, from step 5000 (the crack closed,
//                           compression), a shear increment of 4e-7 in x on
//                           `loading`; per step the contact states, every 10
//                           steps digests of the displacement, the contact
//                           force, the tangential force and the tangential
//                           tractions, full arrays at checkpoints. The
//                           environment CPL_PROBE_FRICTION_STEPS=N
//                           shortens the run to N steps for a
//                           valgrind run (D-093 (2) evidence; never a
//                           golden, the staged description says so); with
//                           it, CPL_PROBE_MEMCHECK_UNPATCHED=1 lets the case
//                           run on the regular build (the negative control)
//   contact_static_friction (UD-374; **UD-001 variant
//                           only**) contact_explicit_static.cc with mu = 0.3:
//                           CouplerSolidContact _static, 100 Newton
//                           iterations, compression steps of -1e-4 until the
//                           first Newton failure (at most 5 steps); digests
//                           of the residual and the solution at every
//                           callback event (per-iterate friction history)
//
// Per step every case records SHA-256 digests (little-endian doubles) of
// the main arrays in a text input `digests`, the Newton iteration counts and
// errors of the static solvers and some energies; at a few checkpoint steps
// and at the end, the full arrays.
//
// Usage: couplers_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: CPL_PROBE_CASES=<id>[,<id>...] restricts the run.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <cohesive_contact_solvercallback.hh>
#include <contact_mechanics_model.hh>
#include <dof_manager.hh>
#include <coupler_solid_cohesive_contact.hh>
#include <coupler_solid_contact.hh>
#include <coupler_solid_phasefield.hh>
#include <material.hh>
#include <material_cohesive.hh>
#include <non_linear_solver.hh>
#include <phase_field_model.hh>
#include <phasefield.hh>
#include <solid_mechanics_model.hh>
#include <solid_mechanics_model_cohesive.hh>
#include <solver_callback.hh>
#include <solver_vector_default.hh>
#include <sparse_matrix_aij.hh>
#include <surface_selector.hh>

#include <Eigen/Core>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check
// (added with version 2, the fixture writer refuses to write
// without it).
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "couplers_probe";
// version 3: four pf_* cases with probe-written material
// files; the cases of versions 1 and 2 are generated unchanged.
// version 4: the case
// friction_cohesive_contact_explicit (UD-001 variant only); the cases of
// versions 1 to 3 are generated unchanged.
constexpr const char * kProbeVersion = "4";

using I64 = std::int64_t;

fs::path g_root, g_up, g_frozen;

bool wanted(const std::string & id) {
  const char * env = std::getenv("CPL_PROBE_CASES");
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

std::vector<double> vec_of(const Array<Real> & a, Int n = -1) {
  auto len = n < 0 ? a.size() * a.getNbComponent() : n;
  return std::vector<double>(a.data(), a.data() + len);
}

std::string hex_digest(const std::vector<double> & v) {
  ap::Sha256 h;
  h.update(v.data(), v.size() * sizeof(double));
  return h.hex();
}

void add_vec(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
             const std::string & desc) {
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
}

void add_ivec(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
              const std::string & desc) {
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
}

void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a))
      .description(desc);
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

void set_newton(NonLinearSolver & s, Int it, Real thr, const std::string & crit) {
  s.set("max_iterations", it);
  s.set("threshold", thr);
  s.set("convergence_type", crit == "solution" ? SolveConvergenceCriteria::_solution
                                               : SolveConvergenceCriteria::_residual);
}

void add_digests(ap::FixtureWriter & w, const std::string & name, const std::string & text) {
  auto tmp = fs::temp_directory_path() / ("couplers_probe_" + name + "_digests.txt");
  std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
  f << text;
  f.close();
  w.add_input_file("digests", tmp);
  fs::remove(tmp);
}

/* -------------------------------------------------------------------------- */
/* solver-callback trace */
/* -------------------------------------------------------------------------- */
/// Forwards every call to the wrapped callback (upstream's
/// InterceptSolverCallback) and reports the events after which the probe
/// records the state: "matrix:<id>", "residual", "corrector".
class TraceCallback : public InterceptSolverCallback {
public:
  TraceCallback(SolverCallback & callback, std::function<void(const std::string &)> on_event)
      : InterceptSolverCallback(callback), on_event(std::move(on_event)) {}
  using InterceptSolverCallback::assembleResidual;
  void assembleMatrix(const ID & matrix_id) override {
    InterceptSolverCallback::assembleMatrix(matrix_id);
    on_event("matrix:" + matrix_id);
  }
  void assembleResidual() override {
    InterceptSolverCallback::assembleResidual();
    on_event("residual");
  }
  void corrector() override {
    InterceptSolverCallback::corrector();
    on_event("corrector");
  }

private:
  std::function<void(const std::string &)> on_event;
};

std::vector<double> vec_of_sv(const SolverVector & v) {
  const auto & a = aka::as_type<SolverVectorArray>(v).getVector();
  return vec_of(a);
}

/// Triplets (0-based rows / columns, values) of a DOF manager's matrix.
void add_triplets(ap::FixtureWriter & w, const std::string & key, DOFManager & dm,
                  const ID & matrix_id) {
  auto & k = aka::as_type<SparseMatrixAIJ>(dm.getMatrix(matrix_id));
  std::vector<I64> irn, jcn;
  std::vector<double> a;
  for (Int i = 0; i < k.getNbNonZero(); ++i) {
    irn.push_back(I64(k.getIRN()(i)) - 1);
    jcn.push_back(I64(k.getJCN()(i)) - 1);
    a.push_back(k.getA()(i));
  }
  add_ivec(w, key + "/irn", irn, matrix_id + " rows (0-based, AIJ order)");
  add_ivec(w, key + "/jcn", jcn, matrix_id + " columns (0-based, AIJ order)");
  add_vec(w, key + "/a", a, matrix_id + " values (AIJ order)");
}

/// (nnz, sum, sum |.|) of a DOF manager's matrix K ({-1, 0, 0} without K).
std::vector<double> k_sums(DOFManager & dm) {
  if (not dm.hasMatrix("K")) {
    return {-1., 0., 0.};
  }
  auto & k = aka::as_type<SparseMatrixAIJ>(dm.getMatrix("K"));
  const auto & a = k.getA();
  double s = 0;
  double sa = 0;
  for (Int i = 0; i < k.getNbNonZero(); ++i) {
    s += a(i);
    sa += std::abs(a(i));
  }
  return {double(k.getNbNonZero()), s, sa};
}

std::vector<I64> contact_elements_of(const ContactMechanicsModel & contact) {
  std::vector<I64> v;
  for (const auto & e : contact.getContactElements()) {
    v.push_back(I64(e.slave));
    v.push_back(I64(e.master.element));
  }
  return v;
}

/* -------------------------------------------------------------------------- */
/* solid - phase field                                                        */
/* -------------------------------------------------------------------------- */
struct NewtonSpec {
  bool set{false};
  Int it{0};
  Real thr{0};
  std::string crit;
};

struct PfCase {
  std::string id, description, msh, dat;
  /// Probe-written material file: written to `<id>.dat` in
  /// the temporary directory and used instead of the upstream `dat`.
  std::string material_text;
  bool explicit_solid{false};
  bool material_selector{false};
  NewtonSpec solid_newton, phase_newton;
  Int steps{0};
  bool save{false};
  std::function<Real(Int)> strain;
  Real bottom{-0.5}, top{0.5};
  std::string solid_id, phase_id;
  std::set<Int> checkpoints;
};

/// applyDisplacement of the upstream tests.
void apply_displacement(SolidMechanicsModel & model, Real eps, Real bottom, Real top) {
  auto & u = model.getDisplacement();
  auto & b = model.getBlockedDOFs();
  const auto & X = model.getMesh().getNodes();
  for (Idx n = 0; n < model.getMesh().getNbNodes(); ++n) {
    if (X(n, 1) == bottom) {
      u(n, 0) = 0;
      u(n, 1) = 0;
      b(n, 0) = true;
      b(n, 1) = true;
    } else if (X(n, 1) == top) {
      u(n, 0) = 0;
      u(n, 1) = eps;
      b(n, 0) = true;
      b(n, 1) = true;
    } else {
      u(n, 0) = 0;
      b(n, 0) = true;
    }
  }
}

/// Damage of material `i` for the regular types of dimension 2 (the first
/// filter-size x nq entries: upstream's unfiltered copy may be longer, its
/// loops read the prefix; the port keeps the prefix), and the stored size.
std::vector<double> material_damage(SolidMechanicsModel & model, Int i, std::vector<I64> * sizes) {
  auto & mat = model.getMaterial(i);
  const auto & cmat = static_cast<const Material &>(mat);
  auto & dam = mat.getInternal<Real>("damage");
  auto & fem = model.getFEEngine();
  std::vector<double> out;
  for (auto type : cmat.getElementFilter().elementTypes(2, _not_ghost)) {
    const auto & a = dam(type, _not_ghost);
    auto n = cmat.getElementFilter(type, _not_ghost).size() * fem.getNbIntegrationPoints(type);
    if (sizes != nullptr) {
      sizes->push_back(a.size());
    }
    auto v = vec_of(a, std::min<Int>(n, a.size()));
    out.insert(out.end(), v.begin(), v.end());
  }
  return out;
}

std::vector<double> material_internal(SolidMechanicsModel & model, Int i, const std::string & name) {
  auto & mat = model.getMaterial(i);
  const auto & cmat = static_cast<const Material &>(mat);
  auto & f = mat.getInternal<Real>(name);
  std::vector<double> out;
  for (auto type : cmat.getElementFilter().elementTypes(2, _not_ghost)) {
    auto v = vec_of(f(type, _not_ghost));
    out.insert(out.end(), v.begin(), v.end());
  }
  return out;
}

std::vector<double> law_internal(PhaseFieldModel & phase, Int l, const std::string & name,
                                 bool previous = false) {
  auto & pf = phase.getPhaseField(l);
  const auto & cpf = static_cast<const PhaseField &>(pf);
  auto & f = pf.getInternal<Real>(name);
  std::vector<double> out;
  for (auto type : cpf.getElementFilter().elementTypes(2, _not_ghost)) {
    auto v = vec_of(previous ? f.previous(type, _not_ghost) : f(type, _not_ghost));
    out.insert(out.end(), v.begin(), v.end());
  }
  return out;
}

void run_pf_case(const PfCase & c) {
  if (not wanted(c.id)) {
    return;
  }
  const std::string cid = "model/couplers/" + c.id;
  const auto dir = g_up / "test/test_model/test_phase_field_model";
  fs::path material = dir / c.dat;
  if (not c.material_text.empty()) {
    material = fs::temp_directory_path() / (c.id + ".dat");
    std::ofstream f(material, std::ios::binary | std::ios::trunc);
    f << c.material_text;
  }
  getStaticParser().parse(material.string());
  Mesh mesh(2);
  mesh.read((dir / c.msh).string());
  CouplerSolidPhaseField coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModel();
  auto & phase = coupler.getPhaseFieldModel();
  if (c.material_selector) {
    solid.setMaterialSelector(
        std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", solid));
  }
  if (c.explicit_solid) {
    solid.initFull(_analysis_method = _explicit_lumped_mass);
  } else {
    solid.initFull(_analysis_method = _static, _solver_options = eigen_options());
  }
  if (c.solid_newton.set) {
    set_newton(solid.getNonLinearSolver(), c.solid_newton.it, c.solid_newton.thr,
               c.solid_newton.crit);
  }
  Real dt = 0;
  if (c.explicit_solid) {
    dt = solid.getStableTimeStep() * 0.8;
    solid.setTimeStep(dt);
  }
  phase.setPhaseFieldSelector(
      std::make_shared<MeshDataPhaseFieldSelector<std::string>>("physical_names", phase));
  phase.initFull(_analysis_method = _static, _solver_options = eigen_options());
  if (c.phase_newton.set) {
    set_newton(phase.getNonLinearSolver(), c.phase_newton.it, c.phase_newton.thr,
               c.phase_newton.crit);
  }

  ap::FixtureWriter w(g_root, cid, c.description, kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("couplers");
  w.add_tag("phase_field");
  w.add_input_file("mesh", dir / c.msh, "upstream:test/test_model/test_phase_field_model/" + c.msh);
  if (c.material_text.empty()) {
    w.add_input_file("material", dir / c.dat,
                     "upstream:test/test_model/test_phase_field_model/" + c.dat);
  } else {
    w.add_input_file("material", material, "probe:material " + c.id);
  }
  auto & p = w.parameters();
  p["explicit_solid"] = c.explicit_solid;
  p["material_selector_physical_names"] = c.material_selector;
  p["phase_field_selector_physical_names"] = true;
  p["steps"] = static_cast<long long>(c.steps);
  p["save_previous_state"] = c.save;
  p["solid_solver_id"] = c.solid_id;
  p["phase_solver_id"] = c.phase_id;
  p["bottom"] = c.bottom;
  p["top"] = c.top;
  p["sparse_solver_type"] = "eigen";
  p["eigen_cache_sizes"] = "16384,524288,524288";
  p["nb_materials"] = static_cast<long long>(solid.getNbMaterials());
  p["nb_phase_fields"] = static_cast<long long>(phase.getNbPhaseFields());
  auto newton_params = [&](const std::string & who, const NewtonSpec & n) {
    p[who + "_newton_set"] = n.set;
    if (n.set) {
      p[who + "_newton_max_iterations"] = static_cast<long long>(n.it);
      p[who + "_newton_threshold"] = n.thr;
      p[who + "_newton_convergence_type"] = n.crit;
    }
  };
  newton_params("solid", c.solid_newton);
  newton_params("phase", c.phase_newton);
  if (c.explicit_solid) {
    add_vec(w, "time_step", {dt}, "0.8 x stable time step of the solid");
  }

  std::vector<double> strains, e_diss, e_pot;
  std::vector<I64> s_it, p_it;
  std::vector<double> s_err, p_err;
  std::stringstream dig;
  Int failed = -1;
  auto record_full = [&](const std::string & prefix) {
    add_array(w, prefix + "/displacement", solid.getDisplacement(), "solid displacement");
    add_array(w, prefix + "/damage", phase.getDamage(), "nodal damage");
    add_array(w, prefix + "/internal_force", solid.getInternalForce(), "solid internal force");
    for (Int i = 0; i < solid.getNbMaterials(); ++i) {
      auto m = prefix + "/material/" + std::to_string(i);
      add_vec(w, m + "/damage", material_damage(solid, i, nullptr),
              "material damage (filter-size prefix)");
      add_vec(w, m + "/stress", material_internal(solid, i, "stress"), "material stress");
      add_vec(w, m + "/grad_u", material_internal(solid, i, "grad_u"), "material grad_u");
    }
    for (Int l = 0; l < phase.getNbPhaseFields(); ++l) {
      auto m = prefix + "/law/" + std::to_string(l);
      add_vec(w, m + "/damage", law_internal(phase, l, "damage"), "phase-field damage internal");
      add_vec(w, m + "/previous_damage", law_internal(phase, l, "damage", true),
              "history of the damage internal");
      add_vec(w, m + "/strain", law_internal(phase, l, "strain"), "strain internal");
      add_vec(w, m + "/phi", law_internal(phase, l, "phi"), "phi internal");
    }
  };
  for (Int s = 0; s < c.steps; ++s) {
    Real eps = c.strain(s);
    strains.push_back(eps);
    apply_displacement(solid, eps, c.bottom, c.top);
    try {
      coupler.solve(c.solid_id, c.phase_id);
    } catch (debug::NLSNotConvergedException & e) {
      failed = s;
      p["failure_step"] = static_cast<long long>(s);
      p["failure_message"] = std::string(e.what());
      add_vec(w, "failure/error", {e.error}, "NLSNotConvergedException::error");
      add_vec(w, "failure/threshold", {e.threshold}, "NLSNotConvergedException::threshold");
      add_ivec(w, "failure/niter", {static_cast<I64>(e.niter)},
               "NLSNotConvergedException::niter");
      break;
    }
    if (not c.explicit_solid) {
      auto & ns = solid.getNonLinearSolver(c.solid_id);
      s_it.push_back(Int(ns.get("nb_iterations")));
      s_err.push_back(Real(ns.get("error")));
    }
    auto & np = phase.getNonLinearSolver(c.phase_id);
    p_it.push_back(Int(np.get("nb_iterations")));
    p_err.push_back(Real(np.get("error")));
    e_diss.push_back(phase.getEnergy("dissipated"));
    e_pot.push_back(solid.getEnergy("potential"));
    if (c.save) {
      phase.savePreviousState();
    }
    std::vector<double> md, ms, ld, ls;
    for (Int i = 0; i < solid.getNbMaterials(); ++i) {
      auto d = material_damage(solid, i, nullptr);
      md.insert(md.end(), d.begin(), d.end());
      auto t = material_internal(solid, i, "stress");
      ms.insert(ms.end(), t.begin(), t.end());
    }
    for (Int l = 0; l < phase.getNbPhaseFields(); ++l) {
      auto d = law_internal(phase, l, "damage");
      ld.insert(ld.end(), d.begin(), d.end());
      auto t = law_internal(phase, l, "strain");
      ls.insert(ls.end(), t.begin(), t.end());
    }
    dig << s << " " << hex_digest(vec_of(solid.getDisplacement())) << " "
        << hex_digest(vec_of(phase.getDamage())) << " " << hex_digest(md) << " "
        << hex_digest(ms) << " " << hex_digest(ld) << " " << hex_digest(ls) << "\n";
    if (c.checkpoints.count(s) > 0) {
      record_full("step_" + std::to_string(s));
    }
  }
  add_vec(w, "schedule/strain", strains, "prescribed eps_yy per step");
  if (not c.explicit_solid) {
    add_ivec(w, "newton/solid_iterations", s_it, "solid nb_iterations per step");
    add_vec(w, "newton/solid_error", s_err, "solid error per step");
  }
  add_ivec(w, "newton/phase_iterations", p_it, "phase-field nb_iterations per step");
  add_vec(w, "newton/phase_error", p_err, "phase-field error per step");
  add_vec(w, "energy/dissipated", e_diss, "phase.getEnergy(\"dissipated\") after each solve");
  add_vec(w, "energy/potential", e_pot, "solid.getEnergy(\"potential\") after each solve");
  add_digests(w, c.id, dig.str());
  {
    std::vector<I64> sizes;
    for (Int i = 0; i < solid.getNbMaterials(); ++i) {
      material_damage(solid, i, &sizes);
    }
    add_ivec(w, "final/material_damage_stored_sizes", sizes,
             "stored size of each material's damage block (per material, per type)");
  }
  record_full("final");
  p["failed"] = failed >= 0;
  w.finish();
  std::cout << "  " << cid << std::endl;
}

void pf_cases() {
  auto ramp = [](Real inc) { return [inc](Int s) { return inc * Real(s); }; };
  auto cycle = [](Real inc) {
    return [inc](Int s) {
      if (s < 500) {
        return inc * Real(s);
      }
      if (s < 1000) {
        return (1500. - 2. * Real(s)) * inc;
      }
      return (3. * Real(s) - 3500.) * inc;
    };
  };
  auto updown = [](Real inc) {
    // test_penalization: eps += inc for s < 500, then eps -= inc
    return [inc](Int s) {
      Real eps = 0.;
      for (Int k = 0; k <= s; ++k) {
        eps += (k < 500) ? inc : -inc;
      }
      return eps;
    };
  };
  const std::set<Int> ck1000{0, 1, 2, 10, 100, 500, 999};
  const std::set<Int> ck1500{0, 1, 10, 499, 500, 750, 999, 1000, 1499};
  std::vector<PfCase> cases;
  {
    PfCase c;
    c.id = "pf_coupling_static";
    c.description = "test_phase_solid_coupling.cc setup through CouplerSolidPhaseField::solve("
                    "\"static\", \"static\"): one element, AT2 no split, eps_yy = 1e-4 s";
    c.msh = "test_one_element.msh";
    c.dat = "material_coupling.dat";
    c.steps = 1000;
    c.strain = ramp(1e-4);
    c.solid_id = "static";
    c.phase_id = "static";
    c.checkpoints = ck1000;
    cases.push_back(c);
  }
  {
    PfCase c;
    c.id = "pf_coupling_explicit";
    c.description = "test_phase_solid_explicit.cc: explicit lumped solid (0.8 x stable step), "
                    "static AT2, eps_yy = 1e-4 s";
    c.msh = "test_one_element.msh";
    c.dat = "material_coupling.dat";
    c.explicit_solid = true;
    c.steps = 1000;
    c.strain = ramp(1e-4);
    c.solid_id = "explicit_lumped";
    c.phase_id = "static";
    c.checkpoints = ck1000;
    cases.push_back(c);
  }
  {
    PfCase c;
    c.id = "pf_deviatoric";
    c.description = "test_phase_field_deviatoric_split.cc: vol/dev split, load / unload into "
                    "compression / reload, savePreviousState";
    c.msh = "test_one_element.msh";
    c.dat = "material_deviatoric.dat";
    c.solid_newton = {true, 1000, 1e-6, "residual"};
    c.phase_newton = {true, 1000, 1e-6, "solution"};
    c.steps = 1500;
    c.save = true;
    c.strain = cycle(1e-5);
    c.solid_id = "static";
    c.phase_id = "static";
    c.checkpoints = ck1500;
    cases.push_back(c);
  }
  {
    PfCase c;
    c.id = "pf_anisotropic";
    c.description = "test_phase_field_anisotropic.cc: material_penalization.dat (AT2, no split, "
                    "penalization), load / unload / reload, savePreviousState";
    c.msh = "test_one_element.msh";
    c.dat = "material_penalization.dat";
    c.solid_newton = {true, 1000, 1e-6, "residual"};
    c.phase_newton = {true, 1000, 1e-6, "residual"};
    c.steps = 1500;
    c.save = true;
    c.strain = cycle(1e-4);
    c.solid_id = "static";
    c.phase_id = "static";
    c.checkpoints = ck1500;
    cases.push_back(c);
  }
  {
    PfCase c;
    c.id = "pf_penalization";
    c.description = "test_penalization.cc: four elements, AT2 penalization, loading to 5e-3 then "
                    "unloading, savePreviousState";
    c.msh = "test_four_elements.msh";
    c.dat = "material_penalization.dat";
    c.solid_newton = {true, 1000, 1e-8, "residual"};
    c.phase_newton = {true, 1000, 1e-6, "solution"};
    c.steps = 1000;
    c.save = true;
    c.strain = updown(1e-5);
    c.solid_id = "static";
    c.phase_id = "static";
    c.checkpoints = ck1000;
    cases.push_back(c);
  }
  {
    PfCase c;
    c.id = "pf_linear";
    c.description = "test_phase_field_linear.cc setup through the coupler: AT1, eps_yy = 1e-4 s";
    c.msh = "test_one_element.msh";
    c.dat = "material_linear.dat";
    c.solid_newton = {true, 100, 1e-9, "residual"};
    c.steps = 1000;
    c.strain = ramp(1e-4);
    c.solid_id = "static";
    c.phase_id = "";
    c.checkpoints = ck1000;
    cases.push_back(c);
  }
  {
    PfCase c;
    c.id = "pf_multi_material";
    c.description = "test_multi_material.cc (UNSTABLE upstream): materials / phase fields plate "
                    "and rigid, explicit solid, default solvers (coupler.solve())";
    c.msh = "test_two_element.msh";
    c.dat = "material_multiple.dat";
    c.explicit_solid = true;
    c.material_selector = true;
    c.steps = 1000;
    c.strain = ramp(1e-4);
    c.bottom = -1.;
    c.top = 1.;
    c.solid_id = "";
    c.phase_id = "";
    c.checkpoints = {0};
    cases.push_back(c);
  }
  {
    PfCase c;
    c.id = "pf_multi_material_converging";
    c.description = "test_multi_material.cc with the phase-field Newton at 1000 iterations / "
                    "1e-10 (solution): upstream's unfiltered damage transfer (UD-382) and "
                    "interpolation (UD-377) on a converging run";
    c.msh = "test_two_element.msh";
    c.dat = "material_multiple.dat";
    c.explicit_solid = true;
    c.material_selector = true;
    c.phase_newton = {true, 1000, 1e-10, "solution"};
    c.steps = 100;
    c.strain = ramp(1e-4);
    c.bottom = -1.;
    c.top = 1.;
    c.solid_id = "";
    c.phase_id = "";
    c.checkpoints = {0, 1, 2, 10, 50, 99};
    cases.push_back(c);
  }
  // the four cells of the acceptance table O4
  // (split x AT1 / AT2 x history / penalization) missing above.
  // Load / unload into compression / reload without a zero-strain step
  // (s + 1 while loading, odd multiples while unloading, 3 s - 3500 never 0).
  auto cycle_nz = [](Real inc) {
    return [inc](Int s) {
      if (s < 500) {
        return inc * Real(s + 1);
      }
      if (s < 1000) {
        return (1501. - 2. * Real(s)) * inc;
      }
      return (3. * Real(s) - 3500.) * inc;
    };
  };
  auto material = [](const std::string & law, const std::string & split,
                     const std::string & irreversibility) {
    std::ostringstream m;
    m << "model solid_mechanics_model [\n"
      << "  material phasefield " << split << " [\n"
      << "    name = plate\n    rho = 1.\n    E = 210.0\n    nu = 0.3\n"
      << "    Plane_Stress = false\n  ]\n]\n\n"
      << "model phase_field_model [\n"
      << "  phasefield " << law << " " << split << " [\n"
      << "    name = plate\n    E = 210.0\n    nu = 0.3\n    gc = 5e-3\n    l0 = 0.3\n"
      << "    Plane_Stress = false\n    irreversibility_type = " << irreversibility << "\n"
      << "    irreversibility_tol = 1e-3\n";
    if (law == "linear") {
      m << "    recovery_tol = 1e-3\n";
    }
    m << "  ]\n]\n";
    return m.str();
  };
  struct Cell {
    std::string id, law, split, irreversibility, label;
  };
  const std::vector<Cell> cells{
      {"pf_linear_penalization", "linear", "no_split", "penalization", "no_split AT1 penalization"},
      {"pf_deviatoric_penalization", "quadratic", "volumetric_deviatoric", "penalization",
       "vol/dev AT2 penalization"},
      {"pf_linear_deviatoric", "linear", "volumetric_deviatoric", "history",
       "vol/dev AT1 history"},
      {"pf_linear_deviatoric_penalization", "linear", "volumetric_deviatoric", "penalization",
       "vol/dev AT1 penalization"},
  };
  for (const auto & cell : cells) {
    PfCase c;
    c.id = cell.id;
    c.description = "acceptance table O4 through CouplerSolidPhaseField: " + cell.label +
                    " (probe material: material phasefield " + cell.split + ", phasefield " +
                    cell.law + " " + cell.split + ", l0 = 0.3), four elements, load / unload "
                    "into compression / reload in steps of 2e-5 without a zero strain, "
                    "savePreviousState";
    c.msh = "test_four_elements.msh";
    c.material_text = material(cell.law, cell.split, cell.irreversibility);
    c.solid_newton = {true, 1000, 1e-8, "residual"};
    c.phase_newton = {true, 1000, 1e-8, "solution"};
    c.steps = 1500;
    c.save = true;
    c.strain = cycle_nz(2e-5);
    c.solid_id = "static";
    c.phase_id = "static";
    c.checkpoints = ck1500;
    cases.push_back(c);
  }
  for (auto & c : cases) {
    run_pf_case(c);
  }
}

/* -------------------------------------------------------------------------- */
/* oracle variant label (as oracle/probes/contact) */
/* -------------------------------------------------------------------------- */
/// Label of a case generated on the UD-001 oracle variant
/// (oracle/patches/variants/ud001-contact-zero-init.patch): variant-only
/// cases refuse to run without it, and the build environment must record the
/// patched files.
void label_variant(ap::FixtureWriter & w) {
  const char * patch = std::getenv("CPL_PROBE_VARIANT_PATCH");
  const char * sha = std::getenv("CPL_PROBE_VARIANT_SHA256");
  const char * defect = std::getenv("CPL_PROBE_VARIANT_DEFECT");
  const char * patched = std::getenv("AKANTU_PROBE_PATCHES");
  if (patch == nullptr or sha == nullptr or defect == nullptr) {
    throw std::runtime_error("variant-only case without CPL_PROBE_VARIANT_{PATCH,SHA256,DEFECT}");
  }
  // the memcheck negative control runs the shortened
  // friction case on the REGULAR build to show UD-001's reads: allowed only
  // with the memcheck step limit, whose staged case is never a golden
  const char * unpatched = std::getenv("CPL_PROBE_MEMCHECK_UNPATCHED");
  const char * steps = std::getenv("CPL_PROBE_FRICTION_STEPS");
  const bool negative_control = unpatched != nullptr and std::string(unpatched) == "1" and
                                steps != nullptr and *steps != '\0';
  if (not negative_control and
      (patched == nullptr or
       std::string(patched).find("resolution_penalty.cc") == std::string::npos or
       std::string(patched).find("contact_detector_inline_impl.hh") == std::string::npos)) {
    throw std::runtime_error("variant-only case on a build without the patched "
                             "resolution_penalty.cc / contact_detector_inline_impl.hh "
                             "(AKANTU_PROBE_PATCHES)");
  }
  ap::Json v = ap::Json::object();
  v["patch"] = std::string(patch);
  v["sha256"] = std::string(sha);
  v["defect"] = std::string(defect);
  w.parameters()["oracle_variant"] = v;
}

bool variant_build() { return std::getenv("CPL_PROBE_VARIANT_PATCH") != nullptr; }

/* -------------------------------------------------------------------------- */
/* solid - contact (static)                                                   */
/* -------------------------------------------------------------------------- */
void contact_static_hertz() {
  const std::string id = "contact_static_hertz";
  if (not wanted(id)) {
    return;
  }
  const std::string cid = "model/couplers/" + id;
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  getStaticParser().parse((g_up / (ex + "material.dat")).string());
  Mesh mesh(2);
  auto mesh_file = g_frozen / (ex + "hertz.msh");
  mesh.read(mesh_file.string());
  CouplerSolidContact coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModel();
  auto & contact = coupler.getContactMechanicsModel();
  solid.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", solid));
  coupler.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto surface_selector = std::make_shared<PhysicalSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "fixed");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "fixed");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "loading");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "symmetry");

  ap::FixtureWriter w(g_root, cid,
                      "examples/c++/contact_mechanics_model/hertz/contact_explicit_static.cc: "
                      "CouplerSolidContact static (Newton, contact search in every corrector), "
                      "100 increments of -1e-4 on loading",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("couplers");
  w.add_tag("contact");
  w.add_input_file("mesh", mesh_file, ex + "hertz.geo");
  w.add_input_file("material", g_up / (ex + "material.dat"), ex + "material.dat");
  auto & p = w.parameters();
  p["steps"] = 100LL;
  p["increment"] = -1e-4;
  p["sparse_solver_type"] = "eigen";
  p["eigen_cache_sizes"] = "16384,524288,524288";
  std::vector<I64> it, nb;
  std::vector<double> err;
  std::stringstream dig;
  const std::set<Int> checkpoints{0, 1, 2, 10, 50, 99};
  auto & nls = coupler.getNonLinearSolver();
  for (Int s = 0; s < 100; ++s) {
    coupler.applyBC(BC::Dirichlet::IncrementValue(-1e-4, _y), "loading");
    coupler.solveStep();
    it.push_back(Int(nls.get("nb_iterations")));
    err.push_back(Real(nls.get("error")));
    nb.push_back(contact.getContactElements().size());
    dig << s << " " << hex_digest(vec_of(solid.getDisplacement())) << " "
        << hex_digest(vec_of(contact.getInternalForce())) << " "
        << hex_digest(vec_of(contact.getGaps())) << "\n";
    if (checkpoints.count(s) > 0) {
      auto pre = "step_" + std::to_string(s);
      add_array(w, pre + "/displacement", solid.getDisplacement(), "displacement");
      add_array(w, pre + "/internal_force", solid.getInternalForce(), "solid internal force");
      add_array(w, pre + "/contact_force", contact.getInternalForce(), "contact force");
      add_array(w, pre + "/gaps", contact.getGaps(), "gaps");
      add_array(w, pre + "/normals", contact.getNormals(), "normals");
      add_array(w, pre + "/areas", contact.getNodalArea(), "nodal areas");
    }
  }
  add_ivec(w, "newton/iterations", it, "nb_iterations per step");
  add_vec(w, "newton/error", err, "error per step");
  add_ivec(w, "nb_contact_elements", nb, "contact elements after each step");
  add_digests(w, id, dig.str());
  add_vec(w, "final/stress_upper", vec_of(solid.getMaterial("upper").getStress(_quadrangle_4)),
          "stress of material upper (quadrangle_4)");
  add_vec(w, "final/stress_lower", vec_of(solid.getMaterial("lower").getStress(_quadrangle_4)),
          "stress of material lower (quadrangle_4)");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* solid - cohesive - contact                                                 */
/* -------------------------------------------------------------------------- */
std::string cohesive_dat(const fs::path & src) {
  std::ifstream f(src);
  std::stringstream ss;
  ss << f.rdbuf();
  auto text = ss.str();
  auto pos = text.find("epsilon_n = 0");
  if (pos == std::string::npos) {
    throw std::runtime_error("material-cohesive.dat: no 'epsilon_n = 0'");
  }
  text.replace(pos, std::string("epsilon_n = 0").size(), "epsilon_n = 1e+03");
  return text;
}

void cohesive_contact_case(bool callback) {
  const std::string id = callback ? "cohesive_contact_callback" : "cohesive_contact_explicit";
  if (not wanted(id)) {
    return;
  }
  const std::string cid = "model/couplers/" + id;
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  auto dat = fs::temp_directory_path() / ("couplers_probe_" + id + "_material.dat");
  {
    std::ofstream f(dat, std::ios::binary | std::ios::trunc);
    f << cohesive_dat(g_up / (ex + "material-cohesive.dat"));
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  auto mesh_file = g_frozen / (ex + "cohesive-contact.msh");
  mesh.read(mesh_file.string());
  CouplerSolidCohesiveContact coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModelCohesive();
  auto & contact = coupler.getContactMechanicsModel();
  auto material_selector = std::make_shared<MeshDataMaterialCohesiveSelector>(solid);
  material_selector->setFallback(solid.getMaterialSelector());
  solid.setMaterialSelector(material_selector);
  auto surface_selector = std::make_shared<CohesiveSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);
  coupler.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "sides");
  Real dt = solid.getStableTimeStep() * 0.1;
  coupler.setTimeStep(dt);

  const Int load = 2500;
  const Int total = callback ? 3000 : 3 * load;
  const Real inc = 1e-3 / Real(load);
  ap::FixtureWriter w(
      g_root, cid,
      std::string("examples/c++/contact_mechanics_model/hertz/"
                  "cohesive_contact_explicit_dynamic.cc with epsilon_n = 1e3, ") +
          std::to_string(load) + " loading steps then unloading (+-1e-3/2500 per step), " +
          std::to_string(total) + " steps" +
          (callback ? ", solved with solid.solveStep(CohesiveContactSolverCallback)"
                    : ", coupler.solveStep(); checkCohesiveStress after each step"),
      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("couplers");
  w.add_tag("cohesive");
  w.add_tag("contact");
  w.add_input_file("mesh", mesh_file, ex + "cohesive-contact.geo");
  w.add_input_file("material", dat, ex + "material-cohesive.dat (epsilon_n = 1e+03)");
  auto & p = w.parameters();
  p["steps"] = static_cast<long long>(total);
  p["loading_steps"] = static_cast<long long>(load);
  p["increment"] = inc;
  p["epsilon_n"] = 1e3;
  p["callback"] = callback;
  add_vec(w, "time_step", {dt}, "0.1 x stable time step");
  std::vector<I64> nb, inserted;
  std::vector<double> epot, ekin;
  std::stringstream dig;
  const std::set<Int> checkpoints{0, 1, 1000, 1999, 2000, 2499, 2500, 2999, 5000, 7499};
  AnalysisMethod method = _explicit_lumped_mass;
  CohesiveContactSolverCallback cb(solid, contact, method);
  I64 n_ins = 0;
  for (Int s = 0; s < total; ++s) {
    Real i = s < load ? inc : -inc;
    coupler.applyBC(BC::Dirichlet::IncrementValue(i, _y), "loading");
    coupler.applyBC(BC::Dirichlet::IncrementValue(-i, _y), "fixed");
    if (callback) {
      solid.solveStep(cb);
    } else {
      coupler.solveStep();
    }
    n_ins += coupler.checkCohesiveStress();
    auto & velocity = solid.getVelocity();
    auto & gaps = contact.getGaps();
    for (auto && [gap, vel] : zip(gaps, make_view(velocity, 2))) {
      if (gap > 0) {
        vel *= 0.99;
      }
    }
    nb.push_back(contact.getContactElements().size());
    inserted.push_back(n_ins);
    if (s % 10 == 0 or s == total - 1) {
      dig << s << " " << hex_digest(vec_of(solid.getDisplacement())) << " "
          << hex_digest(vec_of(contact.getInternalForce())) << "\n";
      epot.push_back(solid.getEnergy("potential"));
      ekin.push_back(solid.getEnergy("kinetic"));
    }
    if (checkpoints.count(s) > 0) {
      auto pre = "step_" + std::to_string(s);
      add_array(w, pre + "/displacement", solid.getDisplacement(), "displacement");
      add_array(w, pre + "/velocity", solid.getVelocity(), "velocity");
      add_array(w, pre + "/contact_force", contact.getInternalForce(), "contact force");
      add_array(w, pre + "/gaps", contact.getGaps(), "gaps");
      add_array(w, pre + "/areas", contact.getNodalArea(), "nodal areas");
    }
  }
  add_ivec(w, "nb_contact_elements", nb, "contact elements after each step");
  add_ivec(w, "inserted", inserted, "cumulative inserted cohesive elements after each step");
  add_vec(w, "energy/potential", epot, "solid potential energy every 10 steps (and the last)");
  add_vec(w, "energy/kinetic", ekin, "solid kinetic energy every 10 steps (and the last)");
  add_digests(w, id, dig.str());
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* solid - cohesive - contact with friction (version 4) */
/* -------------------------------------------------------------------------- */
std::string replace_once(std::string text, const std::string & from, const std::string & to) {
  auto pos = text.find(from);
  if (pos == std::string::npos) {
    throw std::runtime_error("material-cohesive.dat: no '" + from + "'");
  }
  return text.replace(pos, from.size(), to);
}

void cohesive_contact_friction() {
  const std::string id = "friction_cohesive_contact_explicit";
  if (not wanted(id)) {
    return;
  }
  if (not variant_build()) {
    std::cout << "  " << id << ": UD-001 variant only, skipped" << std::endl;
    return;
  }
  const std::string cid = "model/couplers/" + id;
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  auto dat = fs::temp_directory_path() / ("couplers_probe_" + id + "_material.dat");
  {
    std::ifstream f(g_up / (ex + "material-cohesive.dat"));
    std::stringstream ss;
    ss << f.rdbuf();
    auto text = replace_once(ss.str(), "mu = 0.0", "mu = 0.3");
    text = replace_once(text, "epsilon_n = 0", "epsilon_n = 1e+03");
    text = replace_once(text, "epsilon_t = 0", "epsilon_t = 1e+03");
    std::ofstream o(dat, std::ios::binary | std::ios::trunc);
    o << text;
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  auto mesh_file = g_frozen / (ex + "cohesive-contact.msh");
  mesh.read(mesh_file.string());
  CouplerSolidCohesiveContact coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModelCohesive();
  auto & contact = coupler.getContactMechanicsModel();
  auto material_selector = std::make_shared<MeshDataMaterialCohesiveSelector>(solid);
  material_selector->setFallback(solid.getMaterialSelector());
  solid.setMaterialSelector(material_selector);
  auto surface_selector = std::make_shared<CohesiveSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);
  coupler.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "fixed");
  Real dt = solid.getStableTimeStep() * 0.1;
  coupler.setTimeStep(dt);

  const Int load = 2500;
  Int total = 3 * load;
  const Int shear_from = 2 * load;
  const Real inc = 1e-3 / Real(load);
  const Real shear = 1e-3 / Real(load);
  // CPL_PROBE_FRICTION_STEPS=N: the run shortened to
  // its first N steps, for the memory-tool evidence of D-093 (2) only (the
  // golden is the full run; no value is ever taken from a run under a
  // memory tool). The staged case says so and must never be promoted.
  const char * steps_env = std::getenv("CPL_PROBE_FRICTION_STEPS");
  const bool shortened = steps_env != nullptr && *steps_env != '\0';
  if (shortened) {
    total = std::min<Int>(total, std::stoi(steps_env));
  }
  ap::FixtureWriter w(
      g_root, cid,
      std::string(shortened ? "MEMCHECK EVIDENCE RUN, NOT A GOLDEN (shortened to " +
                                  std::to_string(total) + " steps): "
                            : "") +
          "examples/c++/contact_mechanics_model/hertz/cohesive_contact_explicit_dynamic.cc with "
          "Coulomb friction on the crack faces (mu = 0.3, epsilon_n = epsilon_t = 1e3), the "
          "bottom blocked in x instead of the sides, 2500 loading then unloading steps "
          "(+-1e-3/2500 per step in y), 7500 steps, from step 5000 a shear increment of "
          "1e-3/2500 per step in x on loading; coupler.solveStep(), checkCohesiveStress after "
          "each step; UD-001 variant oracle",
      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("couplers");
  w.add_tag("cohesive");
  w.add_tag("contact");
  w.add_tag("friction");
  label_variant(w);
  w.add_input_file("mesh", mesh_file, ex + "cohesive-contact.geo");
  w.add_input_file("material", dat,
                   ex + "material-cohesive.dat (mu = 0.3, epsilon_n = epsilon_t = 1e+03)");
  auto & p = w.parameters();
  p["steps"] = static_cast<long long>(total);
  if (shortened) {
    p["memcheck_evidence_run"] = true;
  }
  p["loading_steps"] = static_cast<long long>(load);
  p["shear_from"] = static_cast<long long>(shear_from);
  p["increment"] = inc;
  p["shear_increment"] = shear;
  p["mu"] = 0.3;
  p["epsilon_n"] = 1e3;
  p["epsilon_t"] = 1e3;
  add_vec(w, "time_step", {dt}, "0.1 x stable time step");
  std::vector<I64> nb, inserted, n_stick, n_slip;
  std::vector<double> epot, ekin;
  std::stringstream dig;
  const std::set<Int> checkpoints{0, 2499, 4999, 5000, 5500, 6000, 7499};
  I64 n_ins = 0;
  for (Int s = 0; s < total; ++s) {
    Real i = s < load ? inc : -inc;
    coupler.applyBC(BC::Dirichlet::IncrementValue(i, _y), "loading");
    coupler.applyBC(BC::Dirichlet::IncrementValue(-i, _y), "fixed");
    if (s >= shear_from) {
      coupler.applyBC(BC::Dirichlet::IncrementValue(shear, _x), "loading");
    }
    coupler.solveStep();
    n_ins += coupler.checkCohesiveStress();
    auto & velocity = solid.getVelocity();
    auto & gaps = contact.getGaps();
    for (auto && [gap, vel] : zip(gaps, make_view(velocity, 2))) {
      if (gap > 0) {
        vel *= 0.99;
      }
    }
    nb.push_back(contact.getContactElements().size());
    inserted.push_back(n_ins);
    I64 stick = 0, slip = 0;
    for (auto && element : contact.getContactElements()) {
      auto state = contact.getContactState()(element.slave);
      stick += (state == ContactState::_stick);
      slip += (state == ContactState::_slip);
    }
    n_stick.push_back(stick);
    n_slip.push_back(slip);
    if (s % 10 == 0 or s == total - 1) {
      dig << s << " " << hex_digest(vec_of(solid.getDisplacement())) << " "
          << hex_digest(vec_of(contact.getInternalForce())) << " "
          << hex_digest(vec_of(contact.getTangentialForce())) << " "
          << hex_digest(vec_of(contact.getTangentialTractions())) << "\n";
      epot.push_back(solid.getEnergy("potential"));
      ekin.push_back(solid.getEnergy("kinetic"));
    }
    if (checkpoints.count(s) > 0) {
      auto pre = "step_" + std::to_string(s);
      add_array(w, pre + "/displacement", solid.getDisplacement(), "displacement");
      add_array(w, pre + "/velocity", solid.getVelocity(), "velocity");
      add_array(w, pre + "/contact_force", contact.getInternalForce(), "contact force");
      add_array(w, pre + "/tangential_force", contact.getTangentialForce(), "tangential force");
      add_array(w, pre + "/gaps", contact.getGaps(), "gaps");
      add_array(w, pre + "/areas", contact.getNodalArea(), "nodal areas");
      add_array(w, pre + "/tangential_tractions", contact.getTangentialTractions(),
                "tangential tractions");
      std::vector<I64> states;
      auto & cs = contact.getContactState();
      for (Idx k = 0; k < cs.size(); ++k) {
        states.push_back(static_cast<I64>(cs(k)));
      }
      add_ivec(w, pre + "/contact_state", states,
               "contact state per node (0 no contact, 1 stick, 2 slip)");
    }
  }
  add_ivec(w, "nb_contact_elements", nb, "contact elements after each step");
  add_ivec(w, "inserted", inserted, "cumulative inserted cohesive elements after each step");
  add_ivec(w, "nb_stick", n_stick, "contact elements whose slave sticks, after each step");
  add_ivec(w, "nb_slip", n_slip, "contact elements whose slave slips, after each step");
  add_vec(w, "energy/potential", epot, "solid potential energy every 10 steps (and the last)");
  add_vec(w, "energy/kinetic", ekin, "solid kinetic energy every 10 steps (and the last)");
  add_digests(w, id, dig.str());
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* solid - cohesive - contact, static (CP-01 / UD-772) */
/* -------------------------------------------------------------------------- */
void cohesive_static() {
  const std::string id = "cohesive_static";
  if (not wanted(id)) {
    return;
  }
  const std::string cid = "model/couplers/" + id;
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  auto dat = fs::temp_directory_path() / ("couplers_probe_" + id + "_material.dat");
  {
    std::ofstream f(dat, std::ios::binary | std::ios::trunc);
    f << cohesive_dat(g_up / (ex + "material-cohesive.dat"));
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  auto mesh_file = g_frozen / (ex + "cohesive-contact.msh");
  mesh.read(mesh_file.string());
  CouplerSolidCohesiveContact coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModelCohesive();
  auto & contact = coupler.getContactMechanicsModel();
  auto material_selector = std::make_shared<MeshDataMaterialCohesiveSelector>(solid);
  material_selector->setFallback(solid.getMaterialSelector());
  solid.setMaterialSelector(material_selector);
  auto surface_selector = std::make_shared<CohesiveSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);
  coupler.initFull(_analysis_method = _static, _is_extrinsic = false,
                   _solver_options = eigen_options());
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "sides");
  auto & nls = coupler.getNonLinearSolver();
  nls.set("max_iterations", 100);
  auto & dm = coupler.getDOFManager();

  const Int max_steps = 10;
  const Real inc = 1e-3 / 50.;
  ap::FixtureWriter w(
      g_root, cid,
      "examples/c++/contact_mechanics_model/hertz/cohesive-contact mesh through "
      "CouplerSolidCohesiveContact _static (intrinsic cohesive_linear of material-cohesive.dat, "
      "epsilon_n = 1e3), compression of 1e-3/50 per step (loading -inc, fixed +inc, sides x "
      "fixed), Newton at 100 iterations, until the first Newton failure (at most 10 steps); "
      "callback trace of steps 0 and 1; composition of K (UD-772)",
      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("couplers");
  w.add_tag("cohesive");
  w.add_tag("contact");
  w.add_input_file("mesh", mesh_file, ex + "cohesive-contact.geo");
  w.add_input_file("material", dat, ex + "material-cohesive.dat (epsilon_n = 1e+03)");
  auto & p = w.parameters();
  p["max_steps"] = static_cast<long long>(max_steps);
  p["increment"] = inc;
  p["epsilon_n"] = 1e3;
  p["max_iterations"] = 100LL;
  p["sparse_solver_type"] = "eigen";
  p["eigen_cache_sizes"] = "16384,524288,524288";
  p["contact_dof_manager_is_coupler_dof_manager"] =
      (&contact.getDOFManager() == &coupler.getDOFManager());
  add_ivec(w, "cohesive_elements", {I64(mesh.getNbElement(_cohesive_2d_4))},
           "intrinsic cohesive elements after initFull");

  // per-event trace of the first two steps
  Int step = 0;
  Int event = 0;
  std::vector<I64> events;
  auto on_event = [&](const std::string & what) {
    if (step > 1) {
      return;
    }
    auto pre = "trace/step_" + std::to_string(step) + "/e" + std::to_string(event);
    if (what == "matrix:K") {
      events.push_back(1);
      add_triplets(w, pre + "/K", dm, "K");
    } else if (what == "residual") {
      events.push_back(2);
      add_vec(w, pre + "/residual", vec_of_sv(dm.getResidual()), "global residual");
    } else if (what == "corrector") {
      events.push_back(3);
      add_triplets(w, pre + "/J", dm, "J");
      add_vec(w, pre + "/solution", vec_of_sv(dm.getSolution()), "global solution");
      add_array(w, pre + "/positions", contact.getContactDetector().getPositions(),
                "contact detector positions (searched)");
      add_array(w, pre + "/gaps", contact.getGaps(), "gaps");
      add_array(w, pre + "/normals", contact.getNormals(), "normals");
      add_ivec(w, pre + "/contact_elements", contact_elements_of(contact),
               "(slave, master facet index) per contact element");
    } else {
      events.push_back(0);
    }
    ++event;
  };
  TraceCallback trace(coupler, on_event);

  std::vector<I64> status, it, nb;
  std::vector<double> err, pen;
  std::stringstream dig;
  for (step = 0; step < max_steps; ++step) {
    event = 0;
    events.clear();
    coupler.applyBC(BC::Dirichlet::IncrementValue(-inc, _y), "loading");
    coupler.applyBC(BC::Dirichlet::IncrementValue(inc, _y), "fixed");
    bool ok = true;
    try {
      static_cast<ModelSolver &>(coupler).solveStep(trace);
    } catch (debug::NLSNotConvergedException & e) {
      ok = false;
    }
    if (step <= 1) {
      add_ivec(w, "trace/step_" + std::to_string(step) + "/events", events,
               "callback events in order: 1 matrix K, 2 residual, 3 corrector");
    }
    Real m = 0;
    for (auto g : contact.getGaps()) {
      m = std::max(m, g);
    }
    status.push_back(ok ? 1 : 0);
    it.push_back(Int(nls.get("nb_iterations")));
    err.push_back(Real(nls.get("error")));
    nb.push_back(contact.getContactElements().size());
    pen.push_back(m);
    dig << step << " " << hex_digest(vec_of(solid.getDisplacement())) << " "
        << hex_digest(vec_of(contact.getInternalForce())) << " "
        << hex_digest(vec_of(contact.getGaps())) << "\n";
    auto pre = "step_" + std::to_string(step);
    add_array(w, pre + "/displacement", solid.getDisplacement(), "displacement");
    add_array(w, pre + "/internal_force", solid.getInternalForce(), "solid internal force");
    add_array(w, pre + "/contact_force", contact.getInternalForce(), "contact force");
    add_array(w, pre + "/gaps", contact.getGaps(), "gaps");
    add_array(w, pre + "/normals", contact.getNormals(), "normals");
    add_array(w, pre + "/areas", contact.getNodalArea(), "nodal areas");
    add_array(w, pre + "/positions", contact.getContactDetector().getPositions(),
              "contact detector positions");
    add_ivec(w, pre + "/contact_elements", contact_elements_of(contact),
             "(slave, master facet index) per contact element");
    if (not ok) {
      ++step;
      break;
    }
  }
  p["steps_run"] = static_cast<long long>(step);
  add_ivec(w, "newton/converged", status, "1 = converged, 0 = Newton failure (run stops)");
  add_ivec(w, "newton/iterations", it, "nb_iterations per step");
  add_vec(w, "newton/error", err, "error per step");
  add_ivec(w, "nb_contact_elements", nb, "contact elements after each step");
  add_vec(w, "max_penetration", pen, "max gap after each step");
  add_digests(w, id, dig.str());

  // K composition (UD-772): the coupler's assembleStiffnessMatrix() puts the
  // solid stiffness into the solved K and the contact stiffness into the
  // contact model's own DOF manager
  coupler.assembleStiffnessMatrix();
  add_vec(w, "K/coupler_after_coupler_assembly", k_sums(dm),
          "(nnz, sum, sum |a|) of the coupler's K after coupler.assembleStiffnessMatrix()");
  add_triplets(w, "K/coupler", dm, "K");
  add_vec(w, "K/contact_dof_manager_after_coupler_assembly", k_sums(contact.getDOFManager()),
          "(nnz, sum, sum |a|) of the contact model's own K (accumulated over every assembly "
          "of the run, never zeroed): evidence of UD-772, not reproduced by the port");
  solid.assembleStiffnessMatrix(true);
  add_vec(w, "K/coupler_after_solid_only_assembly", k_sums(dm),
          "(nnz, sum, sum |a|) of the coupler's K after solid.assembleStiffnessMatrix(true)");
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* cohesive - contact, implicit dynamic */
/* -------------------------------------------------------------------------- */
void cohesive_implicit_dynamic(const std::string & id, Real dt) {
  if (not wanted(id)) {
    return;
  }
  const std::string cid = "model/couplers/" + id;
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  auto dat = fs::temp_directory_path() / ("couplers_probe_" + id + "_material.dat");
  {
    std::ofstream f(dat, std::ios::binary | std::ios::trunc);
    f << cohesive_dat(g_up / (ex + "material-cohesive.dat"));
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  auto mesh_file = g_frozen / (ex + "cohesive-contact.msh");
  mesh.read(mesh_file.string());
  CouplerSolidCohesiveContact coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModelCohesive();
  auto & contact = coupler.getContactMechanicsModel();
  auto material_selector = std::make_shared<MeshDataMaterialCohesiveSelector>(solid);
  material_selector->setFallback(solid.getMaterialSelector());
  solid.setMaterialSelector(material_selector);
  auto surface_selector = std::make_shared<CohesiveSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);
  coupler.initFull(_analysis_method = _implicit_dynamic, _is_extrinsic = false,
                   _solver_options = eigen_options());
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "sides");
  coupler.setTimeStep(dt);
  auto & nls = coupler.getNonLinearSolver();
  nls.set("max_iterations", 100);
  auto & dm = coupler.getDOFManager();

  const Int max_steps = 6;
  const Real inc = 1e-3 / 50.;
  ap::FixtureWriter w(
      g_root, cid,
      "the cohesive_static set-up (cohesive-contact mesh, CouplerSolidCohesiveContact, intrinsic "
      "cohesive_linear, epsilon_n = 1e3, compression of 1e-3/50 per step, Newton at 100 "
      "iterations) with _implicit_dynamic and a fixed time step, until the first Newton failure "
      "(at most 6 steps); per-iterate trace of every step (B-021)",
      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("couplers");
  w.add_tag("cohesive");
  w.add_tag("contact");
  w.add_input_file("mesh", mesh_file, ex + "cohesive-contact.geo");
  w.add_input_file("material", dat, ex + "material-cohesive.dat (epsilon_n = 1e+03)");
  auto & p = w.parameters();
  p["analysis_method"] = "_implicit_dynamic";
  p["time_step"] = dt;
  p["max_steps"] = static_cast<long long>(max_steps);
  p["increment"] = inc;
  p["epsilon_n"] = 1e3;
  p["max_iterations"] = 100LL;
  p["sparse_solver_type"] = "eigen";
  p["eigen_cache_sizes"] = "16384,524288,524288";
  add_vec(w, "input/time_step", {dt}, "time step (exact)");

  Int step = 0;
  std::vector<I64> events;
  std::vector<double> residuals, solutions, displacements;
  Int n_res = 0;
  Int n_corr = 0;
  auto on_event = [&](const std::string & what) {
    if (what == "matrix:K") {
      events.push_back(1);
    } else if (what == "residual") {
      events.push_back(2);
      auto r = vec_of_sv(dm.getResidual());
      residuals.insert(residuals.end(), r.begin(), r.end());
      ++n_res;
    } else if (what == "corrector") {
      events.push_back(3);
      auto x = vec_of_sv(dm.getSolution());
      solutions.insert(solutions.end(), x.begin(), x.end());
      auto u = vec_of(solid.getDisplacement());
      displacements.insert(displacements.end(), u.begin(), u.end());
      ++n_corr;
    } else if (what.rfind("matrix:", 0) == 0) {
      events.push_back(4);
    } else {
      events.push_back(0);
    }
  };
  TraceCallback trace(coupler, on_event);

  const auto ndof = static_cast<std::size_t>(solid.getDisplacement().size() *
                                             solid.getDisplacement().getNbComponent());
  std::vector<I64> status, it, nb;
  std::vector<double> err;
  for (step = 0; step < max_steps; ++step) {
    events.clear();
    residuals.clear();
    solutions.clear();
    displacements.clear();
    n_res = 0;
    n_corr = 0;
    coupler.applyBC(BC::Dirichlet::IncrementValue(-inc, _y), "loading");
    coupler.applyBC(BC::Dirichlet::IncrementValue(inc, _y), "fixed");
    bool ok = true;
    try {
      static_cast<ModelSolver &>(coupler).solveStep(trace);
    } catch (debug::NLSNotConvergedException & e) {
      ok = false;
    }
    auto pre = "step_" + std::to_string(step);
    add_ivec(w, "trace/" + pre + "/events", events,
             "callback events in order: 1 matrix K, 4 other matrix, 2 residual, 3 corrector");
    if (n_res > 0) {
      w.add(std::string("trace/") + pre + "/residual",
            {static_cast<std::size_t>(n_res), ndof}, {"entry", "component"}, "table",
            residuals)
          .description("global residual at every residual event");
    }
    if (n_corr > 0) {
      w.add(std::string("trace/") + pre + "/solution",
            {static_cast<std::size_t>(n_corr), ndof}, {"entry", "component"}, "table",
            solutions)
          .description("global solution (increment) at every corrector event");
      w.add(std::string("trace/") + pre + "/displacement",
            {static_cast<std::size_t>(n_corr), ndof}, {"entry", "component"}, "table",
            displacements)
          .description("displacement after every corrector");
    }
    status.push_back(ok ? 1 : 0);
    it.push_back(Int(nls.get("nb_iterations")));
    err.push_back(Real(nls.get("error")));
    nb.push_back(contact.getContactElements().size());
    add_array(w, pre + "/displacement", solid.getDisplacement(), "displacement");
    add_array(w, pre + "/velocity", solid.getVelocity(), "velocity");
    add_array(w, pre + "/acceleration", solid.getAcceleration(), "acceleration");
    add_array(w, pre + "/internal_force", solid.getInternalForce(), "solid internal force");
    add_array(w, pre + "/contact_force", contact.getInternalForce(), "contact force");
    add_array(w, pre + "/gaps", contact.getGaps(), "gaps");
    add_ivec(w, pre + "/contact_elements", contact_elements_of(contact),
             "(slave, master facet index) per contact element");
    if (not ok) {
      ++step;
      break;
    }
  }
  p["steps_run"] = static_cast<long long>(step);
  add_ivec(w, "newton/converged", status, "1 = converged, 0 = Newton failure (run stops)");
  add_ivec(w, "newton/iterations", it, "nb_iterations per step");
  add_vec(w, "newton/error", err, "error per step");
  add_ivec(w, "nb_contact_elements", nb, "contact elements after each step");
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* solid - contact, static with friction (CP-02 / UD-374) */
/* -------------------------------------------------------------------------- */
void contact_static_friction() {
  const std::string id = "contact_static_friction";
  if (not wanted(id)) {
    return;
  }
  if (not variant_build()) {
    std::cout << "  " << id << ": UD-001 variant only, skipped" << std::endl;
    return;
  }
  const std::string cid = "model/couplers/" + id;
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  auto dat = fs::temp_directory_path() / ("couplers_probe_" + id + "_material.dat");
  {
    std::ifstream f(g_up / (ex + "material.dat"));
    std::stringstream ss;
    ss << f.rdbuf();
    auto text = ss.str();
    auto pos = text.find("mu = 0.0");
    if (pos == std::string::npos) {
      throw std::runtime_error("material.dat: no 'mu = 0.0'");
    }
    text.replace(pos, std::string("mu = 0.0").size(), "mu = 0.3");
    std::ofstream o(dat, std::ios::binary | std::ios::trunc);
    o << text;
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  auto mesh_file = g_frozen / (ex + "hertz.msh");
  mesh.read(mesh_file.string());
  CouplerSolidContact coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModel();
  auto & contact = coupler.getContactMechanicsModel();
  solid.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", solid));
  coupler.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto surface_selector = std::make_shared<PhysicalSurfaceSelector>(mesh);
  contact.getContactDetector().setSurfaceSelector(surface_selector);
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "fixed");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "fixed");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "loading");
  coupler.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "symmetry");
  auto & nls = coupler.getNonLinearSolver();
  nls.set("max_iterations", 100);
  auto & dm = coupler.getDOFManager();

  const Int max_steps = 5;
  ap::FixtureWriter w(g_root, cid,
                      "examples/c++/contact_mechanics_model/hertz/contact_explicit_static.cc "
                      "with mu = 0.3 (UD-374: the static coupler saves the friction history at "
                      "every Newton iterate), increments of -1e-4 on loading until the first "
                      "Newton failure (at most 5 steps, 100 iterations); UD-001 variant oracle",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("couplers");
  w.add_tag("contact");
  w.add_tag("friction");
  label_variant(w);
  w.add_input_file("mesh", mesh_file, ex + "hertz.geo");
  w.add_input_file("material", dat, ex + "material.dat (mu = 0.3)");
  auto & p = w.parameters();
  p["max_steps"] = static_cast<long long>(max_steps);
  p["increment"] = -1e-4;
  p["mu"] = 0.3;
  p["max_iterations"] = 100LL;
  p["sparse_solver_type"] = "eigen";
  p["eigen_cache_sizes"] = "16384,524288,524288";

  Int step = 0;
  std::stringstream events;
  auto on_event = [&](const std::string & what) {
    events << step << " " << what;
    if (what == "residual") {
      events << " " << hex_digest(vec_of_sv(dm.getResidual()));
    } else if (what == "corrector") {
      events << " " << hex_digest(vec_of_sv(dm.getSolution())) << " "
             << hex_digest(vec_of(contact.getGaps())) << " "
             << hex_digest(vec_of(contact.getTangentialTractions())) << " "
             << contact.getContactElements().size();
    }
    events << "\n";
  };
  TraceCallback trace(coupler, on_event);
  std::vector<I64> status, it, nb;
  std::vector<double> err;
  std::stringstream dig;
  for (step = 0; step < max_steps; ++step) {
    coupler.applyBC(BC::Dirichlet::IncrementValue(-1e-4, _y), "loading");
    bool ok = true;
    try {
      static_cast<ModelSolver &>(coupler).solveStep(trace);
    } catch (debug::NLSNotConvergedException & e) {
      ok = false;
    }
    status.push_back(ok ? 1 : 0);
    it.push_back(Int(nls.get("nb_iterations")));
    err.push_back(Real(nls.get("error")));
    nb.push_back(contact.getContactElements().size());
    dig << step << " " << hex_digest(vec_of(solid.getDisplacement())) << " "
        << hex_digest(vec_of(contact.getInternalForce())) << " "
        << hex_digest(vec_of(contact.getGaps())) << "\n";
    auto pre = "step_" + std::to_string(step);
    add_array(w, pre + "/displacement", solid.getDisplacement(), "displacement");
    add_array(w, pre + "/contact_force", contact.getInternalForce(), "contact force");
    add_array(w, pre + "/tangential_force", contact.getTangentialForce(), "tangential force");
    add_array(w, pre + "/gaps", contact.getGaps(), "gaps");
    add_array(w, pre + "/tangential_tractions", contact.getTangentialTractions(),
              "tangential tractions");
    add_array(w, pre + "/positions", contact.getContactDetector().getPositions(),
              "contact detector positions");
    if (not ok) {
      ++step;
      break;
    }
  }
  p["steps_run"] = static_cast<long long>(step);
  add_ivec(w, "newton/converged", status, "1 = converged, 0 = Newton failure (run stops)");
  add_ivec(w, "newton/iterations", it, "nb_iterations per step");
  add_vec(w, "newton/error", err, "error per step");
  add_ivec(w, "nb_contact_elements", nb, "contact elements after each step");
  add_digests(w, id, dig.str());
  {
    auto tmp = fs::temp_directory_path() / ("couplers_probe_" + id + "_events.txt");
    std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
    f << events.str();
    f.close();
    w.add_input_file("events", tmp);
    fs::remove(tmp);
  }
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: couplers_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>\n";
    return 2;
  }
  // D-050: host-independent Eigen blocking (Eigen's own defaults)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  g_root = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  try {
    pf_cases();
    contact_static_hertz();
    cohesive_contact_case(false);
    cohesive_contact_case(true);
    cohesive_static();
    cohesive_implicit_dynamic("cohesive_implicit_dynamic_qs", 1e6);
    cohesive_implicit_dynamic("cohesive_implicit_dynamic_dt0p01", 1e-2);
    contact_static_friction();
    cohesive_contact_friction();
  } catch (std::exception & e) {
    std::cerr << "couplers_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
