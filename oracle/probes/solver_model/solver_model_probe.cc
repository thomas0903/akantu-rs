// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the upstream toy model of
// test/test_model/test_common/test_model_solver (MyModel, included
// unmodified from the reference tree) through ModelSolver / DOFManagerDefault
// / NonLinearSolver{NewtonRaphson,Linear,Lumped} / TimeStepSolverDefault and
// every integration scheme, and records the histories (iteration counts,
// errors, energies, displacements/velocities/accelerations, per-step
// SHA-256 digests, callback traces) for the Rust differential tests.
//
// Cases (fixture area solver_model/, fixture format v1):
//   static_model_solver       test_model_solver.cc with the Eigen solver
//   dynamic_explicit          test_model_solver_dynamic.cc, EXPLICIT=true
//   dynamic_implicit          test_model_solver_dynamic.cc, EXPLICIT=false
//                             (Eigen instead of MUMPS)
//   scheme_*                  the dynamic driver on a 51-node bar with every
//                             Newmark-beta preset / solution type and the
//                             generalized-trapezoidal family (MyModel as a
//                             first-order problem)
//   nl_*                      a non-linear bar (probe-defined subclass of
//                             MyModel: stress E(e + e^3), tangent
//                             EA/L (1 + 3 e^2)) under the Newton-Raphson
//                             variants, convergence criteria, non-convergence
//                             with restore, the linear solver, and the
//                             residual criterion converged at the first test
//   dof_manager_default       test_dof_manager_default.cc (Eigen)
//   dof_manager_hex           test_dof_manager.cc (mesh.msh, hexahedra)
//   solver_switch_shared_j    scenario C (probe v2): a
//                             static and a dynamic solver alternating on one
//                             DOF manager reuse each other's stale "J"
//                             (UD-193)
//   damped_* (probe v3) the dynamic driver on
//                             a probe-defined MyModel subclass with Rayleigh
//                             damping C = a M + b K (element matrices):
//                             r -= C v in the second-order residual, J with
//                             d C (Newmark family) and M + dt/2 C (central
//                             difference); the lumped variant creates C at
//                             setIntegrationScheme but never assembles it
//   new_nodes_event (probe v3) non-linear bar, two
//                             nodes and two segments appended +
//                             NewNodesEvent between two static solves: DOF
//                             sizes, equation numbers, flags, the next solve
//                             (DOF-013)
//   dbltrace_newton_raphson (probe v4, DOF-017 / D-081) the
//                             non-linear bar, 2 load steps of Newton-Raphson
//                             at debug level dblTrace in a scratch working
//                             directory: upstream's "blip" on std::cout and
//                             the files nodes.mtx, A_<k>.mtx, b_<k>.mtx,
//                             x_<k>.mtx (non_linear_solver_newton_raphson.cc
//                             :101-108, 147-163). Runs FIRST in the process:
//                             upstream's function-local `static bool first`
//                             and `static int dump_counter` (advanced at every
//                             Newton iteration, traced or not) start fresh.
//   dbltrace_newton_raphson_np2 (probe v5) the same
//                             traced solve on the bar distributed over TWO
//                             processes (run under `mpirun -np 2`, alone in
//                             its processes): rank 0 builds the bar and
//                             distributes it with MeshPartitionMeshData
//                             (segments 0-4 on rank 0, 5-9 on rank 1), the
//                             Newton solver uses MUMPS (upstream's Eigen
//                             solver factorises each rank's local matrix
//                             alone: unusable on several processes). Both
//                             ranks dump into one scratch directory; each
//                             rank records its own case
//                             solver_model/dbltrace_newton_raphson_np2/rank<r>:
//                             the files it wrote (upstream's names:
//                             `_rank-<r>.mtx` from SolverVectorDistributed /
//                             SparseMatrixAIJ::saveMatrix), the directory's
//                             whole listing, its local nodes (global ids,
//                             flags), iterations and displacements.
//                             NonLinearBar::assembleResidual (below) takes
//                             the internal forces of the rank's own elements
//                             only: it drops the synchronize(_user_1) by
//                             which upstream's MyModel adds the ghost
//                             elements' forces. The residual's gather takes
//                             the owned rows, so the shared node's row
//                             misses the other rank's element: this bar
//                             solves another system than the serial bar on
//                             several processes; the port's replay
//                             reproduces it.
//
// Usage: solver_model_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: SOLVER_MODEL_PROBE_CASES=<id>[,<id>...] restricts the run to
// the listed cases (the others are neither run nor staged). Under
// `mpirun -np 2` only dbltrace_newton_raphson_np2 runs (the other cases are
// one-process cases); on one process that case is skipped.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <communicator.hh>
#include <dof_manager_default.hh>
#include <mesh_events.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_partition_mesh_data.hh>
#include <non_linear_solver.hh>
#include <solver_callback.hh>
#include <solver_vector_default.hh>
#include <sparse_matrix_aij.hh>
#include <time_step_solver.hh>

#include "test_model_solver_my_model.hh"

#include <Eigen/Core>

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "solver_model_probe";
// v2: case solver_switch_shared_j, case filter (the v1 cases are unchanged).
// v3: cases damped_* and new_nodes_event; the dynamic driver also records
// "matrix/C" when C exists (never for the v1/v2 cases); D-050 Eigen cache
// sizes pinned in main (the v1/v2 cases are unchanged).
// v4: case dbltrace_newton_raphson (the earlier cases are unchanged).
// v5: case dbltrace_newton_raphson_np2 (two processes; the earlier cases
// are unchanged).
constexpr const char * kProbeVersion = "5";

using I64 = std::int64_t;

/* -------------------------------------------------------------------------- */
/* helpers                                                                     */
/* -------------------------------------------------------------------------- */
std::vector<double> vec_of(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

/// SHA-256 of the arrays' doubles, default NaNs canonicalised (D-046;
/// as the smm probes and the Rust consumers'
/// `canonical_le_bytes`): identical to the raw bytes when no NaN is present.
std::string digest(const std::vector<const Array<Real> *> & arrays) {
  ap::Sha256 h;
  for (const auto * a : arrays) {
    ap::update_doubles_d046(h, a->data(),
                            static_cast<std::size_t>(a->size() * a->getNbComponent()));
  }
  return h.hex();
}

std::string digest_vec(const std::vector<double> & v) {
  ap::Sha256 h;
  ap::update_doubles_d046(h, v.data(), v.size());
  return h.hex();
}

void add_vec(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
             const std::string & axis, const std::string & layout, const std::string & desc) {
  w.add(name, {v.size()}, {axis}, layout, v).description(desc);
}

void add_table(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
               std::size_t rows, std::size_t cols, const std::string & row_axis,
               const std::string & col_axis, const std::string & layout,
               const std::string & desc) {
  w.add(name, {rows, cols}, {row_axis, col_axis}, layout, v).description(desc);
}

void add_ivec(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
              const std::string & axis, const std::string & desc) {
  w.add(name, {v.size()}, {axis}, "table", v).description(desc);
}

/// Write `text` to a scratch file and attach it as an input of role `role`.
void add_text(ap::FixtureWriter & w, const std::string & role, const std::string & file,
              const std::string & text, const std::string & source = "") {
  auto tmp = fs::temp_directory_path() / ("solver_model_probe_" + file);
  {
    std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
    f << text;
  }
  w.add_input_file(role, tmp, source);
  fs::remove(tmp);
}

/// COO of an AIJ matrix (1-based, as getIRN/getJCN), releases, symmetry.
void record_aij(ap::FixtureWriter & w, const std::string & prefix, const SparseMatrixAIJ & A,
                bool full) {
  const auto nnz = A.getNbNonZero();
  std::vector<I64> rows(nnz), cols(nnz);
  std::vector<double> vals(nnz);
  for (Int k = 0; k < nnz; ++k) {
    rows[k] = A.getIRN()(k);
    cols[k] = A.getJCN()(k);
    vals[k] = A.getA()(k);
  }
  auto & p = w.parameters();
  p[prefix + "/nnz"] = static_cast<long long>(nnz);
  {
    std::string bytes(reinterpret_cast<const char *>(rows.data()), rows.size() * sizeof(I64));
    bytes.append(reinterpret_cast<const char *>(cols.data()), cols.size() * sizeof(I64));
    p[prefix + "/coords_sha256"] = ap::Sha256::of(bytes);
  }
  p[prefix + "/values_sha256"] = digest_vec(vals);
  p[prefix + "/profile_release"] = static_cast<long long>(A.getProfileRelease());
  p[prefix + "/value_release"] = static_cast<long long>(A.getValueRelease());
  p[prefix + "/size"] = static_cast<long long>(A.size());
  p[prefix + "/symmetric"] = A.getMatrixType() == _symmetric;
  if (full) {
    w.add(prefix + "/rows", {rows.size()}, {"nnz"}, "matrix", rows)
        .description("SparseMatrixAIJ::getIRN (1-based global rows, storage order)");
    w.add(prefix + "/cols", {cols.size()}, {"nnz"}, "matrix", cols)
        .description("SparseMatrixAIJ::getJCN (1-based global columns, storage order)");
    w.add(prefix + "/values", {vals.size()}, {"nnz"}, "matrix", vals)
        .description("SparseMatrixAIJ::getA (storage order)");
  }
}

/* -------------------------------------------------------------------------- */
/* meshes (verbatim logic of the upstream tests' genMesh)                      */
/* -------------------------------------------------------------------------- */
/// test_model_solver.cc genMesh (segments only).
void gen_mesh_static(Mesh & mesh, Int nb_nodes) {
  MeshAccessor mesh_accessor(mesh);
  auto & nodes = mesh_accessor.getNodes();
  auto & conn = mesh_accessor.getConnectivity(_segment_2);
  nodes.resize(nb_nodes);
  mesh_accessor.setNbGlobalNodes(nb_nodes);
  for (Int n = 0; n < nb_nodes; ++n) {
    nodes(n, _x) = n * (1. / (nb_nodes - 1));
  }
  conn.resize(nb_nodes - 1);
  for (Int n = 0; n < nb_nodes - 1; ++n) {
    conn(n, 0) = n;
    conn(n, 1) = n + 1;
  }
  mesh_accessor.makeReady();
}

/// test_model_solver_dynamic.cc genMesh (segments, "all" and "border").
void gen_mesh_dynamic(Mesh & mesh, Int nb_nodes) {
  MeshAccessor mesh_accessor(mesh);
  auto & nodes = mesh_accessor.getNodes();
  auto & conn = mesh_accessor.getConnectivity(_segment_2);
  nodes.resize(nb_nodes);
  auto & all = mesh.createNodeGroup("all_nodes");
  for (Int n = 0; n < nb_nodes; ++n) {
    nodes(n, _x) = n * (1. / (nb_nodes - 1));
    all.add(n);
  }
  mesh.createElementGroupFromNodeGroup("all", "all_nodes");
  conn.resize(nb_nodes - 1);
  for (Int n = 0; n < nb_nodes - 1; ++n) {
    conn(n, 0) = n;
    conn(n, 1) = n + 1;
  }
  auto & conn_points = mesh_accessor.getConnectivity(_point_1);
  conn_points.resize(2);
  conn_points(0, 0) = 0;
  conn_points(1, 0) = nb_nodes - 1;
  auto & border = mesh.createElementGroup("border", 0);
  border.add({_point_1, 0, _not_ghost}, true);
  border.add({_point_1, 1, _not_ghost}, true);
  mesh_accessor.makeReady();
}

/* -------------------------------------------------------------------------- */
/* models                                                                      */
/* -------------------------------------------------------------------------- */
/// MyModel with a trace of every solver callback.
class TracedModel : public MyModel {
public:
  TracedModel(Real F, Mesh & mesh, bool lumped) : MyModel(F, mesh, lumped) {}

  std::vector<std::string> trace;
  bool tracing{true};

  void log(const std::string & s) {
    if (tracing) {
      trace.push_back(s);
    }
    if (std::getenv("SOLVER_MODEL_PROBE_TRACE") != nullptr) {
      Real n2 = 0;
      for (auto && u : displacement) {
        n2 += u * u;
      }
      std::cerr << "  hook " << s << " |u|^2=" << n2 << "\n";
    }
  }
  void assembleMatrix(const ID & id) override {
    log("assemble_matrix:" + id);
    MyModel::assembleMatrix(id);
  }
  void assembleLumpedMatrix(const ID & id) override {
    log("assemble_lumped_matrix:" + id);
    MyModel::assembleLumpedMatrix(id);
  }
  void assembleResidual() override {
    log("assemble_residual");
    MyModel::assembleResidual();
  }
  void assembleResidual(const ID & part) override {
    log("assemble_residual:" + part);
    SolverCallback::assembleResidual(part);
  }
  void predictor() override {
    log("predictor");
    MyModel::predictor();
  }
  void corrector() override {
    log("corrector");
    MyModel::corrector();
  }
  void beforeSolveStep() override {
    log("before_solve_step");
    MyModel::beforeSolveStep();
  }
  void afterSolveStep(bool converged) override {
    log(std::string("after_solve_step:") + (converged ? "true" : "false"));
    MyModel::afterSolveStep(converged);
  }

  std::string trace_text() const {
    std::string out;
    for (const auto & t : trace) {
      out += t + "\n";
    }
    return out;
  }
};

/// Non-linear bar (probe-defined): stress = E (e + e^3), tangent stiffness
/// EA/L (1 + 3 e^2) re-assembled at every request (K is never cached).
class NonLinearBar : public TracedModel {
public:
  NonLinearBar(Real F, Mesh & mesh, bool lumped) : TracedModel(F, mesh, lumped) {}

  void assembleMatrix(const ID & id) override {
    if (id != "K") {
      TracedModel::assembleMatrix(id);
      return;
    }
    log("assemble_matrix:K");
    auto & K = this->getDOFManager().getMatrix("K");
    K.zero();
    Array<Real> k_all_el(this->mesh.getNbElement(_segment_2), 4);
    auto k_it = k_all_el.begin(2, 2);
    for (auto && [conn, L] :
         zip(make_view(this->mesh.getConnectivity(_segment_2), 2), this->initial_lengths)) {
      auto n1 = conn(0);
      auto n2 = conn(1);
      Real strain = (this->displacement(n2, _x) - this->displacement(n1, _x)) / L;
      Real kt = E * A / L * (1. + 3. * strain * strain);
      auto & k_el = *k_it;
      k_el(0, 0) = kt;
      k_el(1, 0) = -kt;
      k_el(0, 1) = -kt;
      k_el(1, 1) = kt;
      ++k_it;
    }
    this->getDOFManager().assembleElementalMatricesToMatrix("K", "disp", k_all_el,
                                                            _segment_2);
  }

  void assembleResidual() override {
    log("assemble_residual");
    this->getDOFManager().assembleToResidual("disp", forces);
    internal_forces.zero();
    Array<Real> f_el(this->mesh.getNbElement(_segment_2), 2);
    for (auto && [conn, strain, stress, L, f] :
         zip(make_view(this->mesh.getConnectivity(_segment_2), 2), strains, stresses,
             initial_lengths, make_view(f_el, 2))) {
      auto n1 = conn(0);
      auto n2 = conn(1);
      strain = (this->displacement(n2, _x) - this->displacement(n1, _x)) / L;
      stress = E * (strain + strain * strain * strain);
      auto f_n = A * stress;
      f(0) = -f_n;
      f(1) = f_n;
    }
    this->getDOFManager().assembleElementalArrayLocalArray(f_el, internal_forces, _segment_2,
                                                           _not_ghost);
    this->getDOFManager().assembleToResidual("disp", internal_forces, -1.);
  }
};

/// MyModel with Rayleigh damping (probe-defined): getMatrixType
/// ("C") = _symmetric and C = a M + b K assembled once from the element
/// matrices of MyModel::assembleMass / assembleStiffness, entry by entry in
/// this order: c = a * m + b * k with m = {2, 1} * (rho A L / 6.) and
/// k = {1, -1} * (E A / L).
class DampedBar : public TracedModel {
public:
  DampedBar(Real F, Mesh & mesh, bool lumped, Real a, Real b)
      : TracedModel(F, mesh, lumped), a_m(a), b_k(b) {}

  MatrixType getMatrixType(const ID & id) const override {
    if (id == "C") {
      return _symmetric;
    }
    return TracedModel::getMatrixType(id);
  }

  void assembleMatrix(const ID & id) override {
    if (id != "C") {
      TracedModel::assembleMatrix(id);
      return;
    }
    log("assemble_matrix:C");
    if (is_damping_assembled) {
      return;
    }
    auto & C = this->getDOFManager().getMatrix("C");
    C.zero();
    Array<Real> c_all_el(this->mesh.getNbElement(_segment_2), 4);
    for (auto && [conn, c_el] :
         zip(make_view(this->mesh.getConnectivity(_segment_2), 2), make_view(c_all_el, 2, 2))) {
      Real p1 = this->mesh.getNodes()(conn(0), _x);
      Real p2 = this->mesh.getNodes()(conn(1), _x);
      Real L = std::abs(p2 - p1);
      Real s = rho * A * L / 6.;
      Real m_d = 2. * s;
      Real m_o = 1. * s;
      Real k = E * A / L;
      Real k_o = -1. * k;
      c_el(0, 0) = a_m * m_d + b_k * k;
      c_el(1, 1) = a_m * m_d + b_k * k;
      c_el(0, 1) = a_m * m_o + b_k * k_o;
      c_el(1, 0) = a_m * m_o + b_k * k_o;
    }
    this->getDOFManager().assembleElementalMatricesToMatrix("C", "disp", c_all_el, _segment_2);
    is_damping_assembled = true;
  }

private:
  Real a_m;
  Real b_k;
  bool is_damping_assembled{false};
};

/* -------------------------------------------------------------------------- */
/// Sinusoidal Dirichlet functor of test_model_solver_dynamic.cc.
class Sinusoidal : public BC::Dirichlet::DirichletFunctor {
public:
  Sinusoidal(MyModel & model, Real amplitude, Real pulse_width, Real t)
      : model(model), A(amplitude), k(2 * M_PI / pulse_width), t(t),
        v{std::sqrt(model.E / model.rho)} {}
  using DirichletFunctor::operator();
  void operator()(Idx n, Vector<bool> & /*flags*/, Vector<Real> & disp,
                  const Vector<Real> & coord) override {
    auto x = coord(_x);
    model.velocity(n, _x) = k * v * A * sin(k * (x - v * t));
    disp(_x) = A * cos(k * (x - v * t));
  }

private:
  MyModel & model;
  Real A{1.};
  Real k{2 * M_PI};
  Real t{1.};
  Real v{1.};
};

/* -------------------------------------------------------------------------- */
/* history recorder                                                            */
/* -------------------------------------------------------------------------- */
struct History {
  std::vector<double> energies; // [step][wext, epot, ekin, total, max, min]
  std::vector<I64> n_iter;
  std::vector<double> error;
  std::vector<I64> converged;
  std::vector<I64> threw;
  std::vector<std::string> digests;
  std::vector<double> u_snap, v_snap, a_snap;
  std::vector<long long> snap_steps;

  void snapshot(long long step, const MyModel & m) {
    snap_steps.push_back(step);
    auto push = [](std::vector<double> & dst, const Array<Real> & a) {
      dst.insert(dst.end(), a.data(), a.data() + a.size());
    };
    push(u_snap, m.displacement);
    push(v_snap, m.velocity);
    push(a_snap, m.acceleration);
  }

  void solver_state(NonLinearSolver & nls, bool newton) {
    if (newton) {
      n_iter.push_back(Int(nls.get("nb_iterations")));
      error.push_back(Real(nls.get("error")));
      converged.push_back(bool(nls.get("converged")) ? 1 : 0);
    }
  }

  void write(ap::FixtureWriter & w, std::size_t nb_nodes, bool newton) const {
    auto rows = snap_steps.size();
    if (not energies.empty()) {
      add_table(w, "energies", energies, energies.size() / 6, 6, "step", "component", "table",
                "per step (0 = initial): wext, epot, ekin, total, max_disp, min_disp "
                "(MyModel getters, upstream formulas)");
    }
    if (newton) {
      add_ivec(w, "newton/nb_iterations", n_iter, "step",
               "NonLinearSolver::get(\"nb_iterations\") after each solveStep");
      add_vec(w, "newton/error", error, "step", "table",
              "NonLinearSolver::get(\"error\") after each solveStep");
      add_ivec(w, "newton/converged", converged, "step",
               "NonLinearSolver::get(\"converged\") after each solveStep");
    }
    add_ivec(w, "not_converged_exception", threw, "step",
             "1 if solveStep threw NLSNotConvergedException (the run stops there)");
    add_table(w, "snapshots/displacement", u_snap, rows, nb_nodes, "step", "node", "nodal",
              "displacement at the steps listed in parameters snapshot_steps");
    add_table(w, "snapshots/velocity", v_snap, rows, nb_nodes, "step", "node", "nodal",
              "velocity at the steps listed in parameters snapshot_steps");
    add_table(w, "snapshots/acceleration", a_snap, rows, nb_nodes, "step", "node", "nodal",
              "acceleration at the steps listed in parameters snapshot_steps");
    auto & p = w.parameters();
    p["snapshot_steps"] = ap::Json::array_of(snap_steps);
    p["step_digests"] = ap::Json::array_of(digests);
    p["step_digest_definition"] =
        "SHA-256 of displacement || velocity || acceleration (little-endian binary64, node "
        "order) after each solveStep, step 1 first";
  }
};

void record_energies(History & h, MyModel & m, Real wext, Real epot, Real ekin) {
  Real etot = ekin + epot - wext;
  Real max_disp = 0., min_disp = 0.;
  for (auto && disp : m.displacement) {
    max_disp = std::max(max_disp, disp);
    min_disp = std::min(min_disp, disp);
  }
  h.energies.insert(h.energies.end(), {wext, epot, ekin, etot, max_disp, min_disp});
}

/* -------------------------------------------------------------------------- */
/* dynamic driver (test_model_solver_dynamic.cc main, parametrised)            */
/* -------------------------------------------------------------------------- */
struct DynSpec {
  std::string cid;
  std::string description;
  Int nb_nodes{201};
  Int max_steps{400};
  Real time_step{0.001};
  bool lumped_tss{true};
  IntegrationSchemeType scheme{IntegrationSchemeType::_central_difference};
  IntegrationScheme::SolutionType solution{IntegrationScheme::_acceleration};
  std::map<std::string, Real> scheme_params;
  Int max_iterations{20};
  Int snapshot_every{40};
  bool nonlinear{false};
  bool damped{false};        // v3: DampedBar with C = rayleigh_mass M + rayleigh_stiffness K
  Real rayleigh_mass{0.};
  Real rayleigh_stiffness{0.};
  std::string verified; // upstream .verified file (relative to the source root)
};

std::string solution_name(IntegrationScheme::SolutionType s) {
  switch (s) {
  case IntegrationScheme::_displacement:
    return "displacement";
  case IntegrationScheme::_velocity:
    return "velocity";
  case IntegrationScheme::_acceleration:
    return "acceleration";
  default:
    return "not_defined";
  }
}

std::string scheme_name(IntegrationSchemeType t) {
  std::stringstream s;
  s << t;
  return s.str();
}

/// D-050 pins of the v3 cases (recorded in their parameters).
ap::Json oracle_pins() {
  ap::Json j = ap::Json::object();
  j["eigen_cpu_cache_sizes"] = "setCpuCacheSizes(16384, 524288, 524288) at the start of main";
  const char * ct = std::getenv("OPENBLAS_CORETYPE");
  const char * nt = std::getenv("OPENBLAS_NUM_THREADS");
  j["OPENBLAS_CORETYPE"] = ct != nullptr ? ct : "";
  j["OPENBLAS_NUM_THREADS"] = nt != nullptr ? nt : "";
  return j;
}

void stage(const std::string & cid, const std::string & what) {
  if (std::getenv("SOLVER_MODEL_PROBE_TRACE") != nullptr) {
    std::cerr << "[" << cid << "] " << what << "\n";
  }
}

void dynamic_case(const fs::path & root, const fs::path & up, const DynSpec & spec) {
  stage(spec.cid, "start");
  const Int nb_nodes = spec.nb_nodes;
  const Real time_step = spec.time_step;
  const Real F = -9.81;
  const Real pulse_width = 0.2;
  const Real A = 0.01;
  const bool newton = not spec.lumped_tss;

  Mesh mesh(1);
  gen_mesh_dynamic(mesh, nb_nodes);
  mesh.distribute();

  std::unique_ptr<TracedModel> model_ptr;
  if (spec.nonlinear) {
    model_ptr = std::make_unique<NonLinearBar>(F, mesh, spec.lumped_tss);
  } else if (spec.damped) {
    model_ptr = std::make_unique<DampedBar>(F, mesh, spec.lumped_tss, spec.rayleigh_mass,
                                            spec.rayleigh_stiffness);
  } else {
    model_ptr = std::make_unique<TracedModel>(F, mesh, spec.lumped_tss);
  }
  auto & model = *model_ptr;
  model.forces.zero();
  model.blocked.zero();

  model.applyBC(Sinusoidal(model, A, pulse_width, 0.), "all");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "border");
  auto init_u = vec_of(model.displacement);
  auto init_v = vec_of(model.velocity);

  ModelSolverOptions options;
  if (spec.lumped_tss) {
    options.non_linear_solver_type = NonLinearSolverType::_lumped;
    options.timestep_solver_type = TimeStepSolverType::_dynamic_lumped;
  } else {
    options.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
    options.timestep_solver_type = TimeStepSolverType::_dynamic;
    options.sparse_solver_type = SparseSolverType::_eigen;
  }
  stage(spec.cid, "getNewSolver");
  model.getNewSolver("dynamic", options);
  stage(spec.cid, "setIntegrationScheme");
  model.setIntegrationScheme("dynamic", "disp", spec.scheme, spec.solution);
  stage(spec.cid, "scheme set");
  for (auto && [k, v] : spec.scheme_params) {
    model.getTimeStepSolver("dynamic").getIntegrationScheme("disp").set(k, v);
  }
  model.setTimeStep(time_step);

  History h;
  Real wext = 0.;
  model.getDOFManager().zeroResidual();
  model.assembleResidual();
  Real epot = 0;
  Real ekin = 0;
  Real einit = ekin + epot;
  record_energies(h, model, wext, epot, ekin);
  h.snapshot(0, model);
  auto initial_residual = vec_of(dynamic_cast<SolverVectorDefault &>(
                                     model.getDOFManager().getResidual())
                                     .getVector());

  auto & solver = model.getDOFManager().getNonLinearSolver("dynamic");
  if (newton) {
    solver.set("max_iterations", spec.max_iterations);
  }

  std::vector<double> border_bc; // [step][node 0, node N-1][u, v]
  std::string hooks;
  std::string error_message;
  Int error_step = 0;
  for (Int i = 1; i < spec.max_steps + 1; ++i) {
    model.applyBC(Sinusoidal(model, A, pulse_width, time_step * (i - 1)), "border");
    for (Int n : {Int(0), nb_nodes - 1}) {
      border_bc.push_back(model.displacement(n, _x));
      border_bc.push_back(model.velocity(n, _x));
    }
    stage(spec.cid, "step " + std::to_string(i));
    model.trace.clear();
    model.tracing = (i <= 2);
    bool threw = false;
    try {
      model.solveStep("dynamic");
    } catch (debug::NLSNotConvergedException &) {
      threw = true;
    } catch (debug::Exception & e) {
      error_message = e.what();
      error_step = i;
      break;
    }
    h.threw.push_back(threw ? 1 : 0);
    if (i <= 2) {
      hooks += "step " + std::to_string(i) + "\n" + model.trace_text();
    }
    stage(spec.cid, "energies");
    // central difference creates no K (needed {M, C}): the consistent-mass
    // potential energy is then not available and recorded as 0.
    auto & dm = model.getDOFManager();
    epot = (spec.lumped_tss or dm.hasMatrix("K")) ? model.getPotentialEnergy() : 0.;
    ekin = model.getKineticEnergy();
    wext += model.getExternalWorkIncrement();
    record_energies(h, model, wext, epot, ekin);
    h.energies[h.energies.size() - 3] -= einit;
    h.solver_state(solver, newton);
    h.digests.push_back(digest({&model.displacement, &model.velocity, &model.acceleration}));
    if (i % spec.snapshot_every == 0 or i == spec.max_steps or threw) {
      h.snapshot(i, model);
    }
    if (threw) {
      break;
    }
  }

  ap::FixtureWriter w(root, "solver_model/" + spec.cid, spec.description, kProbe,
                      kProbeVersion);
  auto & p = w.parameters();
  if (not error_message.empty()) {
    w.expect_error("akantu::debug::Exception", error_message.substr(0, 200));
    p["error_step"] = static_cast<long long>(error_step);
  }
  p["driver"] = "dynamic";
  p["nb_nodes"] = static_cast<long long>(nb_nodes);
  p["max_steps"] = static_cast<long long>(spec.max_steps);
  p["time_step"] = time_step;
  p["force"] = F;
  p["pulse_width"] = pulse_width;
  p["amplitude"] = A;
  p["time_step_solver_type"] = spec.lumped_tss ? "dynamic_lumped" : "dynamic";
  p["non_linear_solver_type"] = spec.lumped_tss ? "lumped" : "newton_raphson";
  p["sparse_solver_type"] = spec.lumped_tss ? "none" : "eigen";
  p["integration_scheme"] = scheme_name(spec.scheme);
  p["solution_type"] = solution_name(spec.solution);
  p["max_iterations"] = static_cast<long long>(spec.max_iterations);
  p["nonlinear_bar"] = spec.nonlinear;
  if (spec.damped) {
    p["oracle_pins"] = oracle_pins();
    ap::Json r = ap::Json::object();
    r["mass"] = spec.rayleigh_mass;
    r["stiffness"] = spec.rayleigh_stiffness;
    p["rayleigh"] = r;
  }
  p["model_lumped_energies"] = spec.lumped_tss;
  {
    ap::Json sp = ap::Json::object();
    for (auto && [k, v] : spec.scheme_params) {
      sp[k] = v;
    }
    p["scheme_parameters"] = sp;
  }
  add_vec(w, "inputs/initial_displacement", init_u, "node", "nodal",
          "displacement after applyBC(Sinusoidal(t = 0), \"all\") (glibc sin/cos)");
  add_vec(w, "inputs/initial_velocity", init_v, "node", "nodal",
          "velocity after applyBC(Sinusoidal(t = 0), \"all\")");
  w.add("inputs/border_bc", {border_bc.size() / 4, 2, 2}, {"step", "entry", "component"},
        "table", border_bc)
      .description("per step: [node 0, node N-1] x [displacement, velocity] imposed by "
                   "applyBC(Sinusoidal(t = dt (i - 1)), \"border\") before solveStep");
  add_vec(w, "initial_residual", initial_residual, "dof", "dof",
          "residual of model.assembleResidual() before the first step");
  if (not error_message.empty()) {
    add_text(w, "hooks", spec.cid + "_hooks.txt", model.trace_text());
    w.finish();
    std::cout << "[solver_model_probe] " << spec.cid << " (expected error)\n";
    return;
  }
  h.write(w, std::size_t(nb_nodes), newton);
  add_text(w, "hooks", spec.cid + "_hooks.txt", hooks);
  if (not spec.verified.empty()) {
    w.add_input_file("verified", up / spec.verified, spec.verified);
  }
  if (model.getDOFManager().hasLumpedMatrix("M")) {
    add_vec(w, "lumped_mass",
            vec_of(dynamic_cast<SolverVectorDefault &>(
                       model.getDOFManager().getLumpedMatrix("M"))
                       .getVector()),
            "dof", "dof", "lumped matrix M");
  }
  for (const auto * id : {"K", "M", "C", "J"}) {
    if (model.getDOFManager().hasMatrix(id)) {
      record_aij(w, std::string("matrix/") + id,
                 aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix(id)), true);
    }
  }
  w.finish();
  std::cout << "[solver_model_probe] " << spec.cid << "\n";
}

/* -------------------------------------------------------------------------- */
/* static driver (test_model_solver.cc main)                                   */
/* -------------------------------------------------------------------------- */
void static_model_solver_case(const fs::path & root, const fs::path & up) {
  const Int global_nb_nodes = 100;
  const Real F = -10;
  Mesh mesh(1);
  gen_mesh_static(mesh, global_nb_nodes);
  mesh.distribute();
  TracedModel model(F, mesh, false);
  ModelSolverOptions options;
  options.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  options.timestep_solver_type = TimeStepSolverType::_static;
  options.sparse_solver_type = SparseSolverType::_eigen;
  model.getNewSolver("static", options);
  model.setIntegrationScheme("static", "disp", IntegrationSchemeType::_pseudo_time);
  auto & solver = model.getDOFManager().getNonLinearSolver("static");
  solver.set("max_iterations", 2);
  model.solveStep();

  std::stringstream out; // printResults of test_model_solver.cc
  out << std::setprecision(7);
  out << "node" << ", " << std::setw(8) << "disp" << ", " << std::setw(8) << "force" << ", "
      << std::setw(8) << "blocked" << std::endl;
  for (Int n = 0; n < global_nb_nodes; ++n) {
    out << n << ", " << std::setw(8) << model.displacement(n) << ", " << std::setw(8)
        << model.forces(n) << ", " << std::setw(8) << model.blocked(n) << std::endl;
  }

  ap::FixtureWriter w(root, "solver_model/static_model_solver",
                      "test_model_solver.cc (MyModel, 100 nodes, F = -10, static Newton-Raphson, "
                      "pseudo_time, max_iterations = 2) with the Eigen sparse solver",
                      kProbe, kProbeVersion);
  auto & p = w.parameters();
  p["driver"] = "static";
  p["nb_nodes"] = static_cast<long long>(global_nb_nodes);
  p["force"] = F;
  p["max_iterations"] = 2LL;
  p["sparse_solver_type"] = "eigen";
  p["nb_iterations"] = static_cast<long long>(Int(solver.get("nb_iterations")));
  p["converged"] = bool(solver.get("converged"));
  add_vec(w, "newton/error", {Real(solver.get("error"))}, "entry", "table",
          "NonLinearSolver::get(\"error\")");
  add_vec(w, "displacement", vec_of(model.displacement), "node", "nodal", "final displacement");
  add_vec(w, "residual",
          vec_of(dynamic_cast<SolverVectorDefault &>(model.getDOFManager().getResidual())
                     .getVector()),
          "dof", "dof", "DOF manager residual after solveStep");
  add_vec(w, "solution",
          vec_of(dynamic_cast<SolverVectorDefault &>(model.getDOFManager().getSolution())
                     .getVector()),
          "dof", "dof", "DOF manager solution of the last linear solve");
  for (const auto * id : {"K", "J"}) {
    record_aij(w, std::string("matrix/") + id,
               aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix(id)), true);
  }
  add_text(w, "hooks", "static_hooks.txt", model.trace_text());
  add_text(w, "print_results", "static_print_results.txt", out.str());
  w.add_input_file("verified",
                   up / "test/test_model/test_common/test_model_solver/"
                        "test_model_solver_mumps.verified",
                   "test/test_model/test_common/test_model_solver/"
                   "test_model_solver_mumps.verified");
  w.finish();
  std::cout << "[solver_model_probe] static_model_solver\n";
}

/* -------------------------------------------------------------------------- */
/* non-linear static driver                                                    */
/* -------------------------------------------------------------------------- */
struct NlSpec {
  std::string cid;
  std::string description;
  NonLinearSolverType nls{NonLinearSolverType::_newton_raphson};
  SolveConvergenceCriteria criterion{SolveConvergenceCriteria::_solution};
  Int max_iterations{10};
  Int load_steps{5};
  bool repeat_last{false}; ///< solve the last load once more (residual test at start)
  Real load_increment{-2.};
};

void nl_static_case(const fs::path & root, const NlSpec & spec) {
  const Int nb_nodes = 11;
  const Real dF = spec.load_increment;
  Mesh mesh(1);
  gen_mesh_static(mesh, nb_nodes);
  mesh.distribute();
  NonLinearBar model(0., mesh, false);
  ModelSolverOptions options;
  options.non_linear_solver_type = spec.nls;
  options.timestep_solver_type = TimeStepSolverType::_static;
  options.sparse_solver_type = SparseSolverType::_eigen;
  model.getNewSolver("static", options);
  model.setIntegrationScheme("static", "disp", IntegrationSchemeType::_pseudo_time);
  auto & solver = model.getDOFManager().getNonLinearSolver("static");
  const bool newton = spec.nls != NonLinearSolverType::_linear;
  if (newton) {
    solver.set("max_iterations", spec.max_iterations);
    solver.set("convergence_type", spec.criterion);
  }

  std::vector<I64> n_iter, converged, threw;
  std::vector<double> error, u_hist, r_hist;
  std::string hooks;
  Int total = spec.load_steps + (spec.repeat_last ? 1 : 0);
  for (Int s = 1; s <= total; ++s) {
    Int load = std::min(s, spec.load_steps);
    model.forces(nb_nodes - 1, _x) = dF * load;
    model.trace.clear();
    bool exception = false;
    try {
      model.solveStep();
    } catch (debug::NLSNotConvergedException &) {
      exception = true;
    }
    hooks += "step " + std::to_string(s) + "\n" + model.trace_text();
    threw.push_back(exception ? 1 : 0);
    if (newton) {
      n_iter.push_back(Int(solver.get("nb_iterations")));
      error.push_back(Real(solver.get("error")));
      converged.push_back(bool(solver.get("converged")) ? 1 : 0);
    }
    auto u = vec_of(model.displacement);
    u_hist.insert(u_hist.end(), u.begin(), u.end());
    auto r = vec_of(
        dynamic_cast<SolverVectorDefault &>(model.getDOFManager().getResidual()).getVector());
    r_hist.insert(r_hist.end(), r.begin(), r.end());
    if (exception) {
      break;
    }
  }

  ap::FixtureWriter w(root, "solver_model/" + spec.cid, spec.description, kProbe,
                      kProbeVersion);
  auto & p = w.parameters();
  p["driver"] = "nl_static";
  p["nb_nodes"] = static_cast<long long>(nb_nodes);
  p["load_increment"] = dF;
  p["load_steps"] = static_cast<long long>(spec.load_steps);
  p["repeat_last"] = spec.repeat_last;
  {
    std::stringstream s;
    s << spec.nls;
    p["non_linear_solver_type"] = s.str();
    std::stringstream c;
    c << spec.criterion;
    p["convergence_type"] = c.str();
  }
  p["max_iterations"] = static_cast<long long>(spec.max_iterations);
  p["sparse_solver_type"] = "eigen";
  auto steps = threw.size();
  add_ivec(w, "not_converged_exception", threw, "step",
           "1 if solveStep threw NLSNotConvergedException");
  if (newton) {
    add_ivec(w, "newton/nb_iterations", n_iter, "step", "nb_iterations after each solveStep");
    add_vec(w, "newton/error", error, "step", "table", "error after each solveStep");
    add_ivec(w, "newton/converged", converged, "step", "converged after each solveStep");
  }
  add_table(w, "displacement", u_hist, steps, std::size_t(nb_nodes), "step", "node", "nodal",
            "displacement after each solveStep (after the restore of a failed step)");
  add_table(w, "residual", r_hist, steps, std::size_t(nb_nodes), "step", "dof", "dof",
            "DOF manager residual after each solveStep");
  add_text(w, "hooks", spec.cid + "_hooks.txt", hooks);
  record_aij(w, "matrix/J", aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix("J")),
             true);
  w.finish();
  std::cout << "[solver_model_probe] " << spec.cid << "\n";
}

/* -------------------------------------------------------------------------- */
/* test_dof_manager_default.cc                                                 */
/* -------------------------------------------------------------------------- */
class MySolverCallback : public SolverCallback {
public:
  MySolverCallback(Real F, DOFManagerDefault & dof_manager, Int nb_dofs = 3)
      : dof_manager(dof_manager), displacement(nb_dofs, 1, "disp"), blocked(nb_dofs, 1),
        forces(nb_dofs, 1), nb_dofs(nb_dofs) {
    dof_manager.registerDOFs("disp", displacement, _dst_generic);
    dof_manager.registerBlockedDOFs("disp", blocked);
    displacement.set(0.);
    forces.set(0.);
    blocked.set(false);
    forces(nb_dofs - 1, _x) = F;
    blocked(0, _x) = true;
  }
  void assembleMatrix(const ID & matrix_id) override {
    if (matrix_id != "K") {
      return;
    }
    auto & K = dynamic_cast<SparseMatrixAIJ &>(dof_manager.getMatrix("K"));
    K.zero();
    for (Int i = 1; i < nb_dofs - 1; ++i) {
      K.add(i, i, 2.);
    }
    for (Int i = 0; i < nb_dofs - 1; ++i) {
      K.add(i, i + 1, -1.);
    }
    K.add(0, 0, 1);
    K.add(nb_dofs - 1, nb_dofs - 1, 1);
    K *= nb_dofs - 1;
  }
  MatrixType getMatrixType(const ID & matrix_id) const override {
    if (matrix_id == "K") {
      return _symmetric;
    }
    return _mt_not_defined;
  }
  void assembleLumpedMatrix(const ID &) override {}
  void assembleResidual() override { dof_manager.assembleToResidual("disp", forces); }
  void predictor() override {}
  void corrector() override {}

  DOFManagerDefault & dof_manager;
  Array<Real> displacement;
  Array<bool> blocked;
  Array<Real> forces;
  Int nb_dofs;
};

void dof_manager_default_case(const fs::path & root, const fs::path & up) {
  DOFManagerDefault dof_manager("test_dof_manager");
  MySolverCallback callback(10., dof_manager, 11);
  NonLinearSolver & nls = dof_manager.getNewNonLinearSolver(
      "my_nls", {.non_linear_solver_type = NonLinearSolverType::_linear,
                 .sparse_solver_type = SparseSolverType::_eigen});
  TimeStepSolver & tss = dof_manager.getNewTimeStepSolver("my_tss", TimeStepSolverType::_static,
                                                          nls, callback);
  tss.setIntegrationScheme("disp", IntegrationSchemeType::_pseudo_time);
  tss.solveStep(callback);

  auto tmp = fs::temp_directory_path() / "solver_model_probe_K_dof_manager_default.mtx";
  dof_manager.getMatrix("K").saveMatrix(tmp.string());

  std::stringstream out;
  out << std::setw(8) << "disp"
      << " " << std::setw(8) << "force"
      << " " << std::setw(8) << "blocked" << std::endl;
  for (auto && [disp, force, blocked] :
       zip(callback.displacement, callback.forces, callback.blocked)) {
    out << std::setw(8) << disp << " " << std::setw(8) << force << " " << std::setw(8)
        << std::boolalpha << blocked << std::endl;
  }

  ap::FixtureWriter w(root, "solver_model/dof_manager_default",
                      "test_dof_manager_default.cc (11 generic DOFs, linear non-linear solver, "
                      "pseudo_time) with the Eigen sparse solver",
                      kProbe, kProbeVersion);
  w.parameters()["driver"] = "dof_manager_default";
  add_vec(w, "displacement", vec_of(callback.displacement), "dof", "dof", "final displacement");
  add_vec(w, "residual",
          vec_of(dynamic_cast<SolverVectorDefault &>(dof_manager.getResidual()).getVector()),
          "dof", "dof", "residual after solveStep");
  for (const auto * id : {"K", "J"}) {
    record_aij(w, std::string("matrix/") + id,
               aka::as_type<SparseMatrixAIJ>(dof_manager.getMatrix(id)), true);
  }
  w.add_input_file("save_matrix_K", tmp);
  fs::remove(tmp);
  add_text(w, "print_results", "dof_manager_default_print.txt", out.str());
  w.add_input_file("verified",
                   up / "test/test_model/test_common/test_model_solver/"
                        "test_dof_manager_default.verified",
                   "test/test_model/test_common/test_model_solver/"
                   "test_dof_manager_default.verified");
  w.finish();
  std::cout << "[solver_model_probe] dof_manager_default\n";
}

/* -------------------------------------------------------------------------- */
/* test_dof_manager.cc (default DOF manager, serial)                           */
/* -------------------------------------------------------------------------- */
void dof_manager_hex_case(const fs::path & root, const fs::path & fz) {
  const fs::path rel = "test/test_model/test_common/mesh.msh";
  const auto file = fz / rel;
  Mesh mesh(3);
  mesh.read(file.string());
  mesh.distribute();
  auto nb_nodes = mesh.getNbNodes();
  auto nb_total = mesh.getNbGlobalNodes();
  DOFManagerDefault dof_manager_default(mesh, "dof_manager");
  DOFManager & dof_manager = dof_manager_default; // the gtest uses the base interface

  Array<Real> dof1(nb_nodes, 3), dof2(nb_nodes, 5);
  dof_manager.registerDOFs("dofs1", dof1, _dst_nodal);
  auto size1 = dof_manager.getResidual().size();
  dof_manager.registerDOFs("dofs2", dof2, _dst_nodal);
  auto size2 = dof_manager.getResidual().size();

  // AssembleVector (nodal dofs1 = 1 on local-or-master nodes, dofs2 = 2)
  dof_manager.getResidual().zero();
  for (auto && [n, l] : enumerate(make_view(dof1, 3))) {
    l.set(1. * mesh.isLocalOrMasterNode(n));
  }
  dof2.set(2.);
  dof_manager.assembleToResidual("dofs1", dof1);
  dof_manager.assembleToResidual("dofs2", dof2);
  auto residual =
      vec_of(dynamic_cast<SolverVectorDefault &>(dof_manager.getResidual()).getVector());
  dof1.set(0.);
  dof2.set(0.);
  dof_manager.getArrayPerDOFs("dofs1", dof_manager.getResidual(), dof1);
  dof_manager.getArrayPerDOFs("dofs2", dof_manager.getResidual(), dof2);

  // AssembleMatrixNodal
  auto & K = dof_manager.getNewMatrix("K", _symmetric);
  K.zero();
  Array<Real> el1(mesh.getNbElement(3), 8 * 3 * 8 * 3);
  for (auto && m : make_view(el1, 8 * 3, 8 * 3)) {
    m.set(1.);
  }
  dof_manager.assembleElementalMatricesToMatrix("K", "dofs1", el1, _hexahedron_8);
  Array<Real> el2(mesh.getNbElement(3), 8 * 5 * 8 * 5);
  for (auto && m : make_view(el2, 8 * 5, 8 * 5)) {
    m.set(2.);
  }
  dof_manager.assembleElementalMatricesToMatrix("K", "dofs2", el2, _hexahedron_8);

  // product with a deterministic vector (assembleMatMulVectToArray)
  Array<Real> x1(nb_nodes, 3), y1(nb_nodes, 3);
  for (Int k = 0; k < x1.size() * 3; ++k) {
    x1.data()[k] = 0.25 * (k % 7) - 0.5;
  }
  dof_manager.assembleMatMulVectToArray("dofs1", "K", x1, y1, 1.5);

  ap::FixtureWriter w(root, "solver_model/dof_manager_hex",
                      "test_dof_manager.cc on mesh.msh (hexahedra): nodal registrations, "
                      "AssembleVector, AssembleMatrixNodal, assembleMatMulVectToArray",
                      kProbe, kProbeVersion);
  w.add_input_reference("mesh", "fixtures/meshes/gmsh-4.13.1/" + rel.string(), file);
  auto & p = w.parameters();
  p["driver"] = "dof_manager_hex";
  p["nb_nodes"] = static_cast<long long>(nb_nodes);
  p["nb_global_nodes"] = static_cast<long long>(nb_total);
  p["residual_size_after_dofs1"] = static_cast<long long>(size1);
  p["residual_size_after_dofs2"] = static_cast<long long>(size2);
  p["system_size"] = static_cast<long long>(dof_manager.getSystemSize());
  add_vec(w, "assemble_vector/residual", residual, "dof", "dof",
          "residual after assembleToResidual(dofs1 = 1, dofs2 = 2)");
  add_vec(w, "assemble_vector/dofs1", vec_of(dof1), "dof", "table",
          "getArrayPerDOFs(dofs1, residual)");
  add_vec(w, "assemble_vector/dofs2", vec_of(dof2), "dof", "table",
          "getArrayPerDOFs(dofs2, residual)");
  record_aij(w, "matrix/K", aka::as_type<SparseMatrixAIJ>(K), false);
  add_vec(w, "matmul/x", vec_of(x1), "dof", "table", "x (dofs1 layout)");
  add_vec(w, "matmul/y", vec_of(y1), "dof", "table",
          "assembleMatMulVectToArray(dofs1, K, x, y, 1.5)");
  w.finish();
  std::cout << "[solver_model_probe] dof_manager_hex\n";
}

/* -------------------------------------------------------------------------- */
/* Scenario C: two time-step solvers share "J"                               */
/* -------------------------------------------------------------------------- */
/// A static (pseudo_time) and a dynamic (trapezoidal_rule_2 / displacement)
/// Newton-Raphson solver alternate on one DOF manager of the 21-node MyModel
/// bar (consistent mass, v0 = 0.1 i, F changed between static solves, the
/// last node blocked mid-run). The schemes track only their own source
/// matrices, so each solver reuses the "J" the other one built (UD-193).
void solver_switch_case(const fs::path & root) {
  const std::string cid = "solver_switch_shared_j";
  stage(cid, "start");
  const Int n = 21;
  Mesh mesh(1);
  gen_mesh_static(mesh, n);
  mesh.distribute();
  MyModel model(-1.0, mesh, false);
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  so.timestep_solver_type = TimeStepSolverType::_static;
  so.sparse_solver_type = SparseSolverType::_eigen;
  model.getNewSolver("static", so);
  model.setIntegrationScheme("static", "disp", IntegrationSchemeType::_pseudo_time);
  ModelSolverOptions dyn = so;
  dyn.timestep_solver_type = TimeStepSolverType::_dynamic;
  model.getNewSolver("dynamic", dyn);
  model.setIntegrationScheme("dynamic", "disp", IntegrationSchemeType::_trapezoidal_rule_2,
                             IntegrationScheme::_displacement);
  model.setTimeStep(0.05, "dynamic");
  for (Int i = 0; i < n; ++i) {
    model.velocity(i) = 0.1 * i;
  }
  // tokens: a solver id solves one step; force=<F> sets the end force;
  // block=<node> blocks a node (at its current displacement)
  const std::vector<std::string> tokens = {
      "static",   "dynamic",    "dynamic", "force=-2", "static",  "dynamic",    "block=20",
      "force=-0.5", "static",   "dynamic", "dynamic",  "force=-1.5", "static"};
  std::vector<std::string> solvers;
  std::vector<I64> n_iter, converged, threw, j_value, j_profile;
  std::vector<double> error, u_hist, v_hist, a_hist, r_hist;
  for (const auto & tok : tokens) {
    if (tok.rfind("force=", 0) == 0) {
      model.forces(n - 1, _x) = std::stod(tok.substr(6));
      continue;
    }
    if (tok.rfind("block=", 0) == 0) {
      model.blocked(std::stoi(tok.substr(6)), _x) = true;
      continue;
    }
    stage(cid, "step " + std::to_string(solvers.size() + 1) + " " + tok);
    bool exception = false;
    try {
      model.solveStep(tok);
    } catch (debug::NLSNotConvergedException &) {
      exception = true;
    }
    auto & nls = model.getDOFManager().getNonLinearSolver(tok);
    solvers.push_back(tok);
    threw.push_back(exception ? 1 : 0);
    n_iter.push_back(Int(nls.get("nb_iterations")));
    error.push_back(Real(nls.get("error")));
    converged.push_back(bool(nls.get("converged")) ? 1 : 0);
    const auto & J = aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix("J"));
    j_value.push_back(static_cast<I64>(J.getValueRelease()));
    j_profile.push_back(static_cast<I64>(J.getProfileRelease()));
    auto push = [](std::vector<double> & dst, const Array<Real> & a) {
      dst.insert(dst.end(), a.data(), a.data() + a.size());
    };
    push(u_hist, model.displacement);
    push(v_hist, model.velocity);
    push(a_hist, model.acceleration);
    auto r = vec_of(
        dynamic_cast<SolverVectorDefault &>(model.getDOFManager().getResidual()).getVector());
    r_hist.insert(r_hist.end(), r.begin(), r.end());
  }

  ap::FixtureWriter w(root, "solver_model/" + cid,
                      "review V38 Appendix A scenario C: static (pseudo_time) and dynamic "
                      "(trapezoidal_rule_2 / displacement) Newton-Raphson solvers alternating on "
                      "one DOF manager (21-node MyModel bar, consistent mass, Eigen); each solver "
                      "reuses the J the other one built (UD-193): non-convergence at the static "
                      "step after dynamic steps, divergence of the dynamic steps after a static "
                      "rebuild",
                      kProbe, kProbeVersion);
  auto & p = w.parameters();
  p["driver"] = "solver_switch";
  p["nb_nodes"] = static_cast<long long>(n);
  p["initial_force"] = -1.0;
  p["initial_velocity"] = "0.1 * node";
  p["time_step_dynamic"] = 0.05;
  p["max_iterations"] = 10LL;
  p["sparse_solver_type"] = "eigen";
  p["sequence"] = ap::Json::array_of(tokens);
  p["step_solvers"] = ap::Json::array_of(solvers);
  const auto steps = solvers.size();
  add_ivec(w, "newton/nb_iterations", n_iter, "step", "nb_iterations after each solveStep");
  add_vec(w, "newton/error", error, "step", "table", "error after each solveStep");
  add_ivec(w, "newton/converged", converged, "step", "converged after each solveStep");
  add_ivec(w, "not_converged_exception", threw, "step",
           "1 if solveStep threw NLSNotConvergedException (the run continues)");
  add_ivec(w, "jacobian/value_release", j_value, "step",
           "SparseMatrixAIJ::getValueRelease of J after each solveStep");
  add_ivec(w, "jacobian/profile_release", j_profile, "step",
           "SparseMatrixAIJ::getProfileRelease of J after each solveStep");
  add_table(w, "displacement", u_hist, steps, std::size_t(n), "step", "node", "nodal",
            "displacement after each solveStep (after the restore of a failed step)");
  add_table(w, "velocity", v_hist, steps, std::size_t(n), "step", "node", "nodal",
            "velocity after each solveStep");
  add_table(w, "acceleration", a_hist, steps, std::size_t(n), "step", "node", "nodal",
            "acceleration after each solveStep");
  add_table(w, "residual", r_hist, steps, std::size_t(n), "step", "dof", "dof",
            "DOF manager residual after each solveStep");
  w.finish();
  std::cout << "[solver_model_probe] " << cid << "\n";
}

/// SOLVER_MODEL_PROBE_CASES filter (unset: every case).
/* -------------------------------------------------------------------------- */
/* v3: nodes appended between two solves (DOF-013) */
/* -------------------------------------------------------------------------- */
void record_layout(ap::FixtureWriter & w, DOFManager & dm, const std::string & prefix) {
  auto & p = w.parameters();
  p[prefix + "/system_size"] = static_cast<long long>(dm.getSystemSize());
  p[prefix + "/local_system_size"] = static_cast<long long>(dm.getLocalSystemSize());
  p[prefix + "/pure_local_system_size"] = static_cast<long long>(dm.getPureLocalSystemSize());
  const auto & eq = dm.getLocalEquationsNumbers("disp");
  std::vector<I64> local(eq.data(), eq.data() + eq.size());
  add_ivec(w, prefix + "/local_equation_numbers", local, "dof",
           "getLocalEquationsNumbers(\"disp\")");
  std::vector<I64> global, flags;
  for (Idx l = 0; l < dm.getLocalSystemSize(); ++l) {
    global.push_back(dm.localToGlobalEquationNumber(l));
    flags.push_back(static_cast<I64>(dm.getDOFFlag(l)));
  }
  add_ivec(w, prefix + "/global_equation_numbers", global, "dof",
           "localToGlobalEquationNumber(local) for every local equation");
  add_ivec(w, prefix + "/dof_flags", flags, "dof", "getDOFFlag(local) (NodeFlag value)");
}

void new_nodes_case(const fs::path & root) {
  const std::string cid = "new_nodes_event";
  stage(cid, "start");
  const Int nb_nodes = 11;
  const Int nb_new = 2;
  const Real F = -0.5;
  Mesh mesh(1);
  gen_mesh_static(mesh, nb_nodes);
  mesh.distribute();
  NonLinearBar model(F, mesh, false);
  ModelSolverOptions options;
  options.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  options.timestep_solver_type = TimeStepSolverType::_static;
  options.sparse_solver_type = SparseSolverType::_eigen;
  model.getNewSolver("static", options);
  model.setIntegrationScheme("static", "disp", IntegrationSchemeType::_pseudo_time);
  auto & dm = model.getDOFManager();
  auto & solver = dm.getNonLinearSolver("static");
  solver.set("max_iterations", 20);
  solver.set("threshold", 1e-12);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);

  ap::FixtureWriter w(root, "solver_model/" + cid,
                      "non-linear bar (11 nodes, F = -0.5, static Newton-Raphson, solution "
                      "criterion 1e-12): solve, append two nodes (x_last + 0.1, + 0.2) and "
                      "two segments extending the bar, send NewNodesEvent, resize the model's "
                      "nodal and elemental arrays, block the new end node, solve again "
                      "(Eigen)",
                      kProbe, kProbeVersion);
  auto & p = w.parameters();
  p["driver"] = "new_nodes";
  p["oracle_pins"] = oracle_pins();
  p["nb_nodes"] = static_cast<long long>(nb_nodes);
  p["nb_new_nodes"] = static_cast<long long>(nb_new);
  p["force"] = F;
  p["max_iterations"] = 20LL;
  p["threshold"] = 1e-12;
  p["sparse_solver_type"] = "eigen";
  auto solve = [&](const std::string & prefix) {
    model.trace.clear();
    model.solveStep("static");
    p[prefix + "/nb_iterations"] = static_cast<long long>(Int(solver.get("nb_iterations")));
    p[prefix + "/converged"] = bool(solver.get("converged"));
    add_vec(w, prefix + "/error", {Real(solver.get("error"))}, "entry", "table",
            "NonLinearSolver::get(\"error\")");
    add_vec(w, prefix + "/displacement", vec_of(model.displacement), "node", "nodal",
            "displacement after solveStep");
    add_vec(w, prefix + "/residual",
            vec_of(dynamic_cast<SolverVectorDefault &>(dm.getResidual()).getVector()), "dof",
            "dof", "DOF manager residual after solveStep");
    add_text(w, prefix + "_hooks", cid + "_" + prefix + "_hooks.txt", model.trace_text());
  };
  stage(cid, "solve 1");
  solve("solve_1");
  stage(cid, "layout before");
  record_layout(w, dm, "before");
  stage(cid, "append nodes");
  {
    MeshAccessor mesh_accessor(mesh);
    auto & nodes = mesh_accessor.getNodes();
    const Real x_last = nodes(nb_nodes - 1, _x);
    auto & conn = mesh_accessor.getConnectivity(_segment_2);
    NewNodesEvent event("solver_model_probe");
    for (Int k = 1; k <= nb_new; ++k) {
      nodes.push_back(x_last + k * 0.1);
      event.getList().push_back(nb_nodes - 1 + k);
      // the new nodes must be assembled: SparseMatrixAIJ::clearProfile (called by
      // resizeGlobalArrays) sets size_ = 0 and the size grows only with the
      // assembled entries, so a never-assembled trailing DOF leaves J smaller
      // than the residual and SparseSolverEigen::solve reads out of bounds
      // (segmentation fault observed with two free blocked nodes; UD-652)
      auto e = conn.size();
      conn.resize(e + 1);
      conn(e, 0) = nb_nodes - 2 + k;
      conn(e, 1) = nb_nodes - 1 + k;
    }
    stage(cid, "sendEvent");
    mesh.sendEvent(event);
    stage(cid, "event sent");
  }
  // MyModel does not react to mesh events: the probe resizes its arrays
  for (auto * a : {&model.displacement, &model.velocity, &model.acceleration, &model.forces,
                   &model.internal_forces}) {
    a->resize(nb_nodes + nb_new, 0.);
  }
  // the new end node is blocked: if the set of blocked DOFs did not change,
  // DOFManager::updateGlobalBlockedDofs returns before resizing
  // global_blocked_dofs, and SparseMatrixAIJ::applyBoundary reads beyond its
  // end (assert of the strict build at the next implicit solve; UD-653)
  model.blocked.resize(nb_nodes + nb_new, false);
  model.blocked(nb_nodes + nb_new - 1) = true;
  for (Int k = 1; k <= nb_new; ++k) {
    const auto & X = mesh.getNodes();
    model.initial_lengths.push_back(std::abs(X(nb_nodes - 1 + k, _x) - X(nb_nodes - 2 + k, _x)));
    model.strains.push_back(0.);
    model.stresses.push_back(0.);
  }
  stage(cid, "layout after");
  record_layout(w, dm, "after_event");
  p["after_event/has_matrix_K"] = dm.hasMatrix("K");
  p["after_event/K_nnz"] =
      static_cast<long long>(aka::as_type<SparseMatrixAIJ>(dm.getMatrix("K")).getNbNonZero());
  p["after_event/J_nnz"] =
      static_cast<long long>(aka::as_type<SparseMatrixAIJ>(dm.getMatrix("J")).getNbNonZero());
  stage(cid, "solve 2");
  solve("solve_2");
  stage(cid, "matrices");
  for (const auto * id : {"K", "J"}) {
    record_aij(w, std::string("matrix/") + id,
               aka::as_type<SparseMatrixAIJ>(dm.getMatrix(id)), true);
  }
  w.finish();
  std::cout << "[solver_model_probe] " << cid << "\n";
}

/* -------------------------------------------------------------------------- */
/* DOF-017: the Newton-Raphson dblTrace dumps                                  */
/* -------------------------------------------------------------------------- */
void dbltrace_case(const fs::path & root) {
  const std::string cid = "dbltrace_newton_raphson";
  const Int nb_nodes = 11;
  const Int load_steps = 2;
  const Real dF = -2.;
  Mesh mesh(1);
  gen_mesh_static(mesh, nb_nodes);
  mesh.distribute();
  NonLinearBar model(0., mesh, false);
  ModelSolverOptions options;
  options.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  options.timestep_solver_type = TimeStepSolverType::_static;
  options.sparse_solver_type = SparseSolverType::_eigen;
  model.getNewSolver("static", options);
  model.setIntegrationScheme("static", "disp", IntegrationSchemeType::_pseudo_time);
  auto & solver = model.getDOFManager().getNonLinearSolver("static");
  solver.set("max_iterations", 10);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);

  const fs::path dir = fs::temp_directory_path() / "solver_model_probe_dbltrace";
  fs::remove_all(dir);
  fs::create_directories(dir);
  const fs::path cwd = fs::current_path();
  std::vector<I64> n_iter;
  std::vector<double> u_hist;
  std::ostringstream captured;
  std::ostringstream debug_sink; // the debugger's trace messages are not recorded
  fs::current_path(dir);
  auto * old_cout = std::cout.rdbuf(captured.rdbuf());
  debug::debugger.setOutStream(debug_sink);
  debug::setDebugLevel(dblTrace);
  for (Int s = 1; s <= load_steps; ++s) {
    model.forces(nb_nodes - 1, _x) = dF * s;
    model.solveStep();
    n_iter.push_back(Int(solver.get("nb_iterations")));
    auto u = vec_of(model.displacement);
    u_hist.insert(u_hist.end(), u.begin(), u.end());
  }
  debug::setDebugLevel(dblWarning);
  debug::debugger.setOutStream(std::cerr);
  std::cout.rdbuf(old_cout);
  fs::current_path(cwd);

  ap::FixtureWriter w(root, "solver_model/" + cid,
                      "non-linear bar (11 nodes), 2 load steps of -2, Newton-Raphson (solution "
                      "criterion, Eigen) at debug level dblTrace in a scratch directory: the "
                      "std::cout text and every file the solver wrote (DOF-017)",
                      kProbe, kProbeVersion);
  auto & p = w.parameters();
  p["driver"] = "dbltrace";
  p["nb_nodes"] = static_cast<long long>(nb_nodes);
  p["load_increment"] = dF;
  p["load_steps"] = static_cast<long long>(load_steps);
  p["non_linear_solver_type"] = "newton_raphson";
  p["convergence_type"] = "solution";
  p["max_iterations"] = 10LL;
  p["sparse_solver_type"] = "eigen";
  add_ivec(w, "newton/nb_iterations", n_iter, "step", "nb_iterations after each solveStep");
  add_table(w, "displacement", u_hist, n_iter.size(), std::size_t(nb_nodes), "step", "node",
            "nodal", "displacement after each solveStep");
  add_text(w, "stdout", cid + "_stdout.txt", captured.str());
  std::vector<fs::path> files;
  for (auto & e : fs::directory_iterator(dir)) {
    if (e.is_regular_file()) {
      files.push_back(e.path());
    }
  }
  std::sort(files.begin(), files.end());
  auto list = ap::Json::array();
  for (auto & f : files) {
    const auto name = f.filename().string();
    list.push(name);
    w.add_input_file("trace:" + name, f, "oracle output (dblTrace dump)");
  }
  p["trace_files"] = list;
  w.finish();
  fs::remove_all(dir);
  std::cout << "[solver_model_probe] " << cid << " (" << files.size() << " files)\n";
}

/* -------------------------------------------------------------------------- */
/* The dblTrace dumps on a mesh distributed over two processes */
/* -------------------------------------------------------------------------- */
void dbltrace_np2_case(const fs::path & root) {
  const std::string cid = "dbltrace_newton_raphson_np2";
  auto & comm = Communicator::getStaticCommunicator();
  const Int prank = comm.whoAmI();
  const Int psize = comm.getNbProc();
  if (psize != 2) {
    throw std::runtime_error(cid + " needs exactly 2 processes");
  }
  const Int nb_nodes = 11;
  const Int load_steps = 2;
  const Real dF = -2.;
  // segments 0-4 on rank 0, 5-9 on rank 1 (the element's global number on
  // rank 0 before the distribution)
  std::vector<I64> parts(nb_nodes - 1);
  for (Int e = 0; e < nb_nodes - 1; ++e) {
    parts[e] = e < (nb_nodes - 1) / 2 ? 0 : 1;
  }
  Mesh mesh(1);
  std::shared_ptr<MeshPartition> partition;
  if (prank == 0) {
    gen_mesh_static(mesh, nb_nodes);
    auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("dbltrace_np2_partition");
    auto & a = mapping->alloc(mesh.getNbElement(_segment_2), 1, _segment_2, _not_ghost);
    for (Int e = 0; e < nb_nodes - 1; ++e) {
      a(e) = Idx(parts[e]);
    }
    partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, 1);
    partition->partitionate(psize);
  }
  mesh.distribute(partition);
  NonLinearBar model(0., mesh, false);
  ModelSolverOptions options;
  options.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  options.timestep_solver_type = TimeStepSolverType::_static;
  options.sparse_solver_type = SparseSolverType::_mumps;
  model.getNewSolver("static", options);
  model.setIntegrationScheme("static", "disp", IntegrationSchemeType::_pseudo_time);
  auto & solver = model.getDOFManager().getNonLinearSolver("static");
  solver.set("max_iterations", 10);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);
  const bool residual_distributed = model.getDOFManager().getResidual().isDistributed();
  const bool solution_distributed = model.getDOFManager().getSolution().isDistributed();

  // one scratch directory for both processes (the names are per rank)
  const fs::path dir = fs::temp_directory_path() / "solver_model_probe_dbltrace_np2";
  if (prank == 0) {
    fs::remove_all(dir);
    fs::create_directories(dir);
  }
  comm.barrier();
  const fs::path cwd = fs::current_path();
  std::vector<I64> n_iter;
  std::vector<double> u_hist;
  std::ostringstream captured;
  std::ostringstream debug_sink; // the debugger's trace messages are not recorded
  fs::current_path(dir);
  auto * old_cout = std::cout.rdbuf(captured.rdbuf());
  debug::debugger.setOutStream(debug_sink);
  debug::setDebugLevel(dblTrace);
  for (Int s = 1; s <= load_steps; ++s) {
    for (Int n = 0; n < mesh.getNbNodes(); ++n) {
      if (mesh.getNodeGlobalId(n) == nb_nodes - 1) {
        model.forces(n, _x) = dF * s;
      }
    }
    model.solveStep();
    n_iter.push_back(Int(solver.get("nb_iterations")));
    auto u = vec_of(model.displacement);
    u_hist.insert(u_hist.end(), u.begin(), u.end());
  }
  debug::setDebugLevel(dblWarning);
  debug::debugger.setOutStream(std::cerr);
  std::cout.rdbuf(old_cout);
  fs::current_path(cwd);
  comm.barrier(); // every rank's files are written

  ap::FixtureWriter w(root, "solver_model/" + cid + "/rank" + std::to_string(prank),
                      "non-linear bar (11 nodes) distributed over 2 processes "
                      "(MeshPartitionMeshData: segments 0-4 on rank 0, 5-9 on rank 1), 2 load "
                      "steps of -2, Newton-Raphson (solution criterion, MUMPS) at debug level "
                      "dblTrace in one scratch directory shared by the ranks: this rank's "
                      "std::cout text, the files it wrote and the directory's listing (review "
                      "V161 LR20-01, DOF-017), rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  auto & p = w.parameters();
  p["driver"] = "dbltrace_np2";
  p["rank"] = static_cast<long long>(prank);
  p["nb_ranks"] = static_cast<long long>(psize);
  p["nb_nodes"] = static_cast<long long>(nb_nodes);
  p["load_increment"] = dF;
  p["load_steps"] = static_cast<long long>(load_steps);
  p["non_linear_solver_type"] = "newton_raphson";
  p["convergence_type"] = "solution";
  p["max_iterations"] = 10LL;
  p["sparse_solver_type"] = "mumps";
  p["partition/kind"] = "MeshPartitionMeshData";
  p["is_distributed"] = mesh.isDistributed();
  p["residual_is_distributed"] = residual_distributed;
  p["solution_is_distributed"] = solution_distributed;
  add_ivec(w, "partition/element_rank", parts, "element",
           "rank of each segment (global element order on rank 0 before the distribution)");
  {
    std::vector<I64> gids, flags;
    for (Int n = 0; n < mesh.getNbNodes(); ++n) {
      gids.push_back(mesh.getNodeGlobalId(n));
      flags.push_back(static_cast<I64>(static_cast<std::uint8_t>(mesh.getNodeFlag(n))));
    }
    add_ivec(w, "nodes/global_id", gids, "node", "global id of each local node");
    add_ivec(w, "nodes/flag", flags, "node",
             "NodeFlag of each local node (upstream's values: 0 normal, 3 master, 5 slave, "
             "9 pure ghost)");
  }
  add_ivec(w, "newton/nb_iterations", n_iter, "step", "nb_iterations after each solveStep");
  add_table(w, "displacement", u_hist, n_iter.size(), std::size_t(mesh.getNbNodes()), "step",
            "node", "nodal", "displacement of the local nodes after each solveStep");
  add_text(w, "stdout", cid + "_rank" + std::to_string(prank) + "_stdout.txt", captured.str());
  std::vector<fs::path> files;
  auto all = ap::Json::array();
  {
    std::vector<std::string> names;
    for (auto & e : fs::directory_iterator(dir)) {
      if (e.is_regular_file()) {
        names.push_back(e.path().filename().string());
      }
    }
    std::sort(names.begin(), names.end());
    const std::string mine = "_rank-" + std::to_string(prank) + ".mtx";
    for (auto & name : names) {
      all.push(name);
      if (name.size() >= mine.size() &&
          name.compare(name.size() - mine.size(), mine.size(), mine) == 0) {
        files.push_back(dir / name);
      }
    }
  }
  auto list = ap::Json::array();
  for (auto & f : files) {
    const auto name = f.filename().string();
    list.push(name);
    w.add_input_file("trace:" + name, f, "oracle output (dblTrace dump, this rank)");
  }
  p["trace_files"] = list;
  p["all_trace_files"] = all;
  w.finish();
  comm.barrier(); // both ranks recorded their files
  if (prank == 0) {
    fs::remove_all(dir);
  }
  std::cout << "[solver_model_probe] " << cid << " rank " << prank << " (" << files.size()
            << " files)\n";
}

bool wanted(const std::string & cid) {
  const char * env = std::getenv("SOLVER_MODEL_PROBE_CASES");
  if (env == nullptr || *env == '\0') {
    return true;
  }
  std::stringstream list(env);
  std::string item;
  while (std::getline(list, item, ',')) {
    if (item == cid) {
      return true;
    }
  }
  return false;
}

} // namespace

/* -------------------------------------------------------------------------- */
int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: solver_model_probe <fixtures_root> <upstream_src_root> "
                 "<frozen_meshes_root>\n";
    return 2;
  }
  const fs::path root = argv[1], up = argv[2], fz = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  // D-050: Eigen's own default cache sizes (host-independent blocking)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  initialize(aka_argc, aka_argv_p);
  RandomGenerator<Idx>::seed(1);
  try {
    // two processes: only the distributed trace case, alone in its
    // processes (upstream's dblTrace statics are fresh)
    if (Communicator::getStaticCommunicator().getNbProc() > 1) {
      if (wanted("dbltrace_newton_raphson_np2")) {
        dbltrace_np2_case(root);
      }
      finalize();
      return 0;
    }
    // first: upstream's dblTrace statics (first call, dump counter) are fresh
    if (wanted("dbltrace_newton_raphson")) {
      dbltrace_case(root);
    }
    if (wanted("static_model_solver")) {
      static_model_solver_case(root, up);
    }
    const std::string vdir = "test/test_model/test_common/test_model_solver/";
    {
      DynSpec s;
      s.cid = "dynamic_explicit";
      s.description = "test_model_solver_dynamic.cc EXPLICIT=true (central difference, "
                      "lumped mass, acceleration solution, 201 nodes, 400 steps)";
      s.verified = vdir + "test_model_solver_dynamic_explicit.verified";
      if (wanted(s.cid)) {
        dynamic_case(root, up, s);
      }
    }
    {
      DynSpec s;
      s.cid = "dynamic_implicit";
      s.description = "test_model_solver_dynamic.cc EXPLICIT=false (trapezoidal_rule_2, "
                      "displacement solution, Newton-Raphson, Eigen instead of MUMPS)";
      s.lumped_tss = false;
      s.scheme = IntegrationSchemeType::_trapezoidal_rule_2;
      s.solution = IntegrationScheme::_displacement;
      s.verified = vdir + "test_model_solver_dynamic_implicit.verified";
      if (wanted(s.cid)) {
        dynamic_case(root, up, s);
      }
    }
    struct V {
      std::string cid;
      bool lumped;
      IntegrationSchemeType t;
      IntegrationScheme::SolutionType s;
      std::map<std::string, Real> params;
    };
    using T = IntegrationSchemeType;
    using S = IntegrationScheme;
    const std::vector<V> variants = {
        {"scheme_central_difference_lumped_acceleration", true, T::_central_difference,
         S::_acceleration, {}},
        {"scheme_central_difference_acceleration", false, T::_central_difference,
         S::_acceleration, {}},
        {"scheme_trapezoidal_rule_2_displacement", false, T::_trapezoidal_rule_2,
         S::_displacement, {}},
        {"scheme_trapezoidal_rule_2_velocity", false, T::_trapezoidal_rule_2, S::_velocity, {}},
        {"scheme_trapezoidal_rule_2_acceleration", false, T::_trapezoidal_rule_2,
         S::_acceleration, {}},
        {"scheme_fox_goodwin_displacement", false, T::_fox_goodwin, S::_displacement, {}},
        {"scheme_fox_goodwin_acceleration", false, T::_fox_goodwin, S::_acceleration, {}},
        {"scheme_linear_acceleration_velocity", false, T::_linear_acceleration, S::_velocity,
         {}},
        {"scheme_newmark_beta_custom_displacement", false, T::_newmark_beta, S::_displacement,
         {{"alpha", 0.4}, {"beta", 0.6}}},
        {"scheme_forward_euler_lumped_temperature_rate", true, T::_forward_euler,
         S::_temperature_rate, {}},
        {"scheme_forward_euler_temperature_rate", false, T::_forward_euler,
         S::_temperature_rate, {}},
        {"scheme_trapezoidal_rule_1_temperature", false, T::_trapezoidal_rule_1, S::_temperature,
         {}},
        {"scheme_trapezoidal_rule_1_temperature_rate", false, T::_trapezoidal_rule_1,
         S::_temperature_rate, {}},
        {"scheme_backward_euler_temperature", false, T::_backward_euler, S::_temperature, {}},
        {"scheme_generalized_trapezoidal_custom_temperature", false,
         T::_generalized_trapezoidal, S::_temperature, {{"alpha", 0.75}}},
    };
    for (const auto & v : variants) {
      DynSpec s;
      s.cid = v.cid;
      s.description = "dynamic driver of test_model_solver_dynamic.cc on a 51-node bar, 60 steps: " +
                      scheme_name(v.t) + " / " + solution_name(v.s) +
                      (v.lumped ? " (dynamic_lumped, lumped solver)"
                                : " (dynamic, Newton-Raphson, Eigen)");
      s.nb_nodes = 51;
      s.max_steps = 60;
      s.snapshot_every = 20;
      s.lumped_tss = v.lumped;
      s.scheme = v.t;
      s.solution = v.s;
      s.scheme_params = v.params;
      if (wanted(s.cid)) {
        dynamic_case(root, up, s);
      }
    }
    {
      DynSpec s;
      s.cid = "nl_dynamic_not_converged_restore";
      s.description = "non-linear bar, trapezoidal_rule_2 / displacement, Newton-Raphson with "
                      "max_iterations = 1: every step fails, afterSolveStep(false) restores";
      s.nb_nodes = 21;
      s.max_steps = 1;
      s.snapshot_every = 1;
      s.lumped_tss = false;
      s.scheme = T::_trapezoidal_rule_2;
      s.solution = S::_displacement;
      s.max_iterations = 1;
      s.nonlinear = true;
      if (wanted(s.cid)) {
        dynamic_case(root, up, s);
      }
    }
    using N = NonLinearSolverType;
    using C = SolveConvergenceCriteria;
    const std::vector<NlSpec> nl = {
        {"nl_newton_raphson_solution", "non-linear bar, 5 load steps, Newton-Raphson, solution "
                                       "criterion",
         N::_newton_raphson, C::_solution, 10, 5, false},
        {"nl_newton_raphson_residual", "non-linear bar, Newton-Raphson, residual criterion "
                                       "(relative after the first test)",
         N::_newton_raphson, C::_residual, 10, 5, false},
        {"nl_newton_raphson_residual_mass_wgh",
         "non-linear bar (load -0.1), Newton-Raphson, residual_mass_wgh criterion: the "
         "solution-norm branch never refreshes the residual, so every iteration re-applies the "
         "first increment and the step fails after max_iterations = 3 (NLS-014)",
         N::_newton_raphson, C::_residual_mass_wgh, 3, 1, false, -0.1},
        {"nl_newton_raphson_modified_solution",
         "non-linear bar (load steps of -0.2), modified Newton-Raphson (J once per step, at "
         "the start state), solution criterion",
         N::_newton_raphson_modified, C::_solution, 60, 5, false, -0.2},
        {"nl_newton_raphson_contact_solution",
         "non-linear bar, newton_raphson_contact type, solution criterion", N::_newton_raphson_contact,
         C::_solution, 10, 5, false},
        {"nl_linear", "non-linear bar, linear solver (one solve per step)", N::_linear,
         C::_solution, 10, 5, false},
        {"nl_newton_raphson_not_converged",
         "non-linear bar, Newton-Raphson with max_iterations = 3: NLSNotConvergedException and "
         "restore of the pseudo-time state",
         N::_newton_raphson, C::_solution, 3, 5, false},
        {"nl_residual_converged_at_first_test",
         "non-linear bar, residual criterion, the last load solved twice: the second solve "
         "converges at the initial test and returns without afterSolveStep (UD-010)",
         N::_newton_raphson, C::_residual, 10, 2, true},
    };
    for (const auto & s : nl) {
      if (wanted(s.cid)) {
        nl_static_case(root, s);
      }
    }
    if (wanted("dof_manager_default")) {
      dof_manager_default_case(root, up);
    }
    if (wanted("dof_manager_hex")) {
      dof_manager_hex_case(root, fz);
    }
    if (wanted("solver_switch_shared_j")) {
      solver_switch_case(root);
    }
    // v3: Rayleigh damping (TS-004, TS-005, TS-006)
    struct D {
      std::string cid;
      bool lumped;
      IntegrationSchemeType t;
      IntegrationScheme::SolutionType s;
      std::map<std::string, Real> params;
    };
    const std::vector<D> damped = {
        {"damped_trapezoidal_rule_2_displacement", false, T::_trapezoidal_rule_2,
         S::_displacement, {}},
        {"damped_newmark_beta_custom_velocity", false, T::_newmark_beta, S::_velocity,
         {{"alpha", 0.4}, {"beta", 0.6}}},
        {"damped_central_difference_acceleration", false, T::_central_difference,
         S::_acceleration, {}},
        {"damped_central_difference_lumped_acceleration", true, T::_central_difference,
         S::_acceleration, {}},
    };
    for (const auto & v : damped) {
      DynSpec s;
      s.cid = v.cid;
      s.description = "dynamic driver of test_model_solver_dynamic.cc on a 51-node bar with "
                      "Rayleigh damping C = 0.5 M + 0.005 K (probe-defined DampedBar), 60 "
                      "steps: " +
                      scheme_name(v.t) + " / " + solution_name(v.s) +
                      (v.lumped ? " (dynamic_lumped, lumped solver)"
                                : " (dynamic, Newton-Raphson, Eigen)");
      s.nb_nodes = 51;
      s.max_steps = 60;
      s.snapshot_every = 20;
      s.lumped_tss = v.lumped;
      s.scheme = v.t;
      s.solution = v.s;
      s.scheme_params = v.params;
      s.damped = true;
      s.rayleigh_mass = 0.5;
      s.rayleigh_stiffness = 0.005;
      if (wanted(s.cid)) {
        dynamic_case(root, up, s);
      }
    }
    // v3: nodes appended between two solves (DOF-013)
    if (wanted("new_nodes_event")) {
      new_nodes_case(root);
    }
  } catch (debug::Exception & e) {
    std::cerr << "solver_model_probe: " << e.info() << "\n";
    finalize();
    return 1;
  } catch (std::exception & e) {
    std::cerr << "solver_model_probe: " << e.what() << "\n";
    finalize();
    return 1;
  }
  finalize();
  return 0;
}
