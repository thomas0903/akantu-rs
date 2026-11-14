// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the upstream PETSc DOF manager, sparse solver and
// non-linear solvers (DOFManagerPETSc, SparseSolverPETSc,
// NonLinearSolverPETSc, NonLinearSolverTAO) on the toy model of
// test/test_model/test_common/test_model_solver (MyModel, included
// unmodified from the reference tree) and prints one JSON document with the
// results; every double is recorded as its IEEE-754 bit pattern (16 hex
// digits) next to a %.17g rendering, so the Rust side compares bits, never
// parsed decimals.
//
// Cases (one process):
//   diag            test_sparse_solver_petsc.cc (11 nodes, A_ii = 1/(1+g),
//                   b = 1, SparseSolverPETSc with default options)
//   static_default  test_model_solver.cc with DOF_MANAGER_TYPE="petsc"
//                   (NR, max_iterations 2, default KSP)
//   dynamic_default test_model_solver_dynamic.cc, EXPLICIT=false,
//                   DOF_MANAGER_TYPE="petsc" (400 steps, default KSP)
//   bar17_*         a 17-node bar (MyModel, F = -10 linear / -2 cubic with
//                   the probe's NonLinearBar), static:
//                     nr_cubic_default   NR (max_iterations 40), default KSP
//                     nr_cubic_direct    NR, KSP preonly + LU
//                     snes_linear_direct petsc_snes, KSP preonly + LU,
//                                        snes_rtol 1e-12, snes_atol 1e-14,
//                                        snes_stol 0, snes_max_it 50
//                     snes_cubic_direct  same with the cubic law
//                     tao_linear_direct  petsc_tao, KSP preonly + LU,
//                                        tao_gatol 1e-12, tao_grtol 0
//
// Usage: petsc_probe            (prints the JSON on rank 0's stdout)
//
// Host independence (D-050): Eigen's cache sizes are
// pinned to Eigen's own defaults (L1 16 KiB, L2 = L3 512 KiB) at the start
// of main, and the OpenBLAS core type / thread count are pinned by the
// environment the generator passes in (OPENBLAS_CORETYPE=Haswell,
// OPENBLAS_NUM_THREADS=1 on x86_64); the record reports what Eigen and
// OpenBLAS actually use ("host" block), plus the PETSc version.

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <dof_manager_petsc.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <non_linear_solver.hh>
#include <non_linear_solver_newton_raphson.hh>
#include <solver_callback.hh>
#include <sparse_solver_petsc.hh>
#include <terms_to_assemble.hh>

#include "test_model_solver_my_model.hh"

#include <Eigen/Core>
#include <dlfcn.h>
#include <petscversion.h>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;

namespace {

std::string hex(double x) {
  std::uint64_t b;
  std::memcpy(&b, &x, sizeof b);
  char buf[32];
  std::snprintf(buf, sizeof buf, "%016llx", static_cast<unsigned long long>(b));
  return buf;
}

std::string dec(double x) {
  char buf[40];
  std::snprintf(buf, sizeof buf, "%.17g", x);
  return buf;
}

std::string json_str(const std::string & s) {
  std::string out = "\"";
  for (char c : s) {
    if (c == '"' || c == '\\') {
      out += '\\';
      out += c;
    } else if (static_cast<unsigned char>(c) < 0x20) {
      char buf[8];
      std::snprintf(buf, sizeof buf, "\\u%04x", static_cast<unsigned>(c));
      out += buf;
    } else {
      out += c;
    }
  }
  return out + "\"";
}

/// `{"bits": [...], "values": [...]}` of a vector of doubles.
std::string reals(const std::vector<double> & v) {
  std::ostringstream o;
  o << "{\"bits\": [";
  for (std::size_t i = 0; i < v.size(); ++i) {
    o << (i ? ", " : "") << '"' << hex(v[i]) << '"';
  }
  o << "], \"values\": [";
  for (std::size_t i = 0; i < v.size(); ++i) {
    o << (i ? ", " : "") << dec(v[i]);
  }
  o << "]}";
  return o.str();
}

std::vector<double> vec_of(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

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

/// The cubic bar of oracle/probes/solver_model (stress E(e + e^3), tangent
/// EA/L (1 + 3 e^2)).
class NonLinearBar : public MyModel {
public:
  NonLinearBar(Real F, Mesh & mesh, bool lumped, const ID & dm)
      : MyModel(F, mesh, lumped, dm) {}

  void assembleMatrix(const ID & id) override {
    if (id != "K") {
      MyModel::assembleMatrix(id);
      return;
    }
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

/// test_sparse_solver_petsc (serial path).
std::string case_diag() {
  const Int n = 11;
  Mesh mesh(1);
  gen_mesh_static(mesh, n);
  mesh.distribute();
  DOFManagerPETSc dof_manager(mesh, "test_dof_manager");
  Array<Real> x(n);
  x.set(-1);
  dof_manager.registerDOFs("x", x, _dst_nodal);
  const auto & leq = dof_manager.getLocalEquationsNumbers("x");
  auto & A = dof_manager.getNewMatrix("A", _symmetric);
  (void)A;
  TermsToAssemble terms("x", "x");
  for (Int i = 0; i < n; ++i) {
    if (dof_manager.isLocalOrMasterDOF(i)) {
      auto li = leq(i);
      auto gi = dof_manager.localToGlobalEquationNumber(li);
      terms(i, i) = 1. / (1. + gi);
    }
  }
  dof_manager.assemblePreassembledMatrix("A", terms);
  Array<Real> b(n);
  b.set(1.);
  SparseSolverPETSc solver(dof_manager, "A");
  dof_manager.assembleToResidual("x", b);
  solver.solve();
  auto & y = dof_manager.getSolution("x");
  return "{\"x\": " + reals(vec_of(y)) + "}";
}

/// test_model_solver_petsc.
std::string case_static_default() {
  Mesh mesh(1);
  gen_mesh_static(mesh, 100);
  mesh.distribute();
  MyModel model(-10, mesh, false, "petsc");
  ModelSolverOptions options;
  options.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  options.timestep_solver_type = TimeStepSolverType::_static;
  model.getNewSolver("static", options);
  model.setIntegrationScheme("static", "disp", IntegrationSchemeType::_pseudo_time);
  auto & solver = model.getDOFManager().getNonLinearSolver("static");
  solver.set("max_iterations", 2);
  model.solveStep();
  std::ostringstream o;
  o << "{\"displacement\": " << reals(vec_of(model.displacement))
    << ", \"n_iter\": " << Int(solver.get("nb_iterations"))
    << ", \"error\": " << reals({Real(solver.get("error"))}) << "}";
  return o.str();
}

/// test_model_solver_dynamic_petsc.
std::string case_dynamic_default() {
  Int global_nb_nodes = 201;
  Int max_steps = 400;
  Real time_step = 0.001;
  Mesh mesh(1);
  Real F = -9.81;
  const Real pulse_width = 0.2;
  const Real A = 0.01;
  gen_mesh_dynamic(mesh, global_nb_nodes);
  mesh.distribute();
  MyModel model(F, mesh, false, "petsc");
  model.forces.zero();
  model.blocked.zero();
  model.applyBC(Sinusoidal(model, A, pulse_width, 0.), "all");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "border");
  ModelSolverOptions options;
  options.non_linear_solver_type = NonLinearSolverType::_newton_raphson;
  options.timestep_solver_type = TimeStepSolverType::_dynamic;
  model.getNewSolver("dynamic", options);
  model.setIntegrationScheme("dynamic", "disp", IntegrationSchemeType::_trapezoidal_rule_2,
                             IntegrationScheme::_displacement);
  model.setTimeStep(time_step);
  Real wext = 0.;
  model.getDOFManager().zeroResidual();
  model.assembleResidual();
  Real epot = 0;
  Real ekin = 0;
  Real einit = ekin + epot;
  std::vector<double> energies;
  std::vector<double> errors;
  std::vector<long> iters;
  auto record = [&](Real wext, Real epot, Real ekin, Real etot) {
    Real max_disp = 0., min_disp = 0.;
    for (auto && disp : model.displacement) {
      max_disp = std::max(max_disp, disp);
      min_disp = std::min(min_disp, disp);
    }
    for (auto v : {wext, epot, ekin, etot, max_disp, min_disp}) {
      energies.push_back(v);
    }
  };
  record(wext, epot, ekin, ekin + epot - wext - einit);
  auto & solver = model.getDOFManager().getNonLinearSolver("dynamic");
  solver.set("max_iterations", 20);
  for (Int i = 1; i < max_steps + 1; ++i) {
    model.applyBC(Sinusoidal(model, A, pulse_width, time_step * (i - 1)), "border");
    model.solveStep("dynamic");
    epot = model.getPotentialEnergy();
    ekin = model.getKineticEnergy();
    wext += model.getExternalWorkIncrement();
    record(wext, epot, ekin, ekin + epot - wext - einit);
    iters.push_back(Int(solver.get("nb_iterations")));
    errors.push_back(Real(solver.get("error")));
  }
  std::ostringstream o;
  o << "{\"energies\": " << reals(energies) << ", \"error\": " << reals(errors)
    << ", \"n_iter\": [";
  for (std::size_t i = 0; i < iters.size(); ++i) {
    o << (i ? ", " : "") << iters[i];
  }
  o << "], \"displacement\": " << reals(vec_of(model.displacement))
    << ", \"velocity\": " << reals(vec_of(model.velocity))
    << ", \"acceleration\": " << reals(vec_of(model.acceleration)) << "}";
  return o.str();
}

/// A bar17 case.
std::string case_bar17(bool cubic, NonLinearSolverType nls,
                       const std::vector<std::pair<std::string, std::string>> & opts,
                       bool ksp_direct_on_sparse_solver) {
  Mesh mesh(1);
  gen_mesh_static(mesh, 17);
  mesh.distribute();
  std::unique_ptr<MyModel> model;
  if (cubic) {
    model = std::make_unique<NonLinearBar>(-2., mesh, false, "petsc");
  } else {
    model = std::make_unique<MyModel>(-10., mesh, false, "petsc");
  }
  ModelSolverOptions options;
  options.non_linear_solver_type = nls;
  options.timestep_solver_type = TimeStepSolverType::_static;
  options.sparse_solver_type = SparseSolverType::_petsc;
  model->getNewSolver("s", options);
  model->setIntegrationScheme("s", "disp", IntegrationSchemeType::_pseudo_time);
  auto & solver = model->getDOFManager().getNonLinearSolver("s");
  if (nls == NonLinearSolverType::_newton_raphson) {
    solver.set("max_iterations", 40);
    if (ksp_direct_on_sparse_solver) {
      auto & ss = aka::as_type<NonLinearSolverNewtonRaphson>(solver).getSparseSolver();
      ss.set("ksp_type", std::string("preonly"));
      ss.set("pc_type", std::string("lu"));
    }
  }
  for (auto && [k, v] : opts) {
    solver.set(k, v);
  }
  std::string status = "ok";
  try {
    model->solveStep();
  } catch (std::exception & e) {
    status = e.what();
  }
  std::ostringstream o;
  o << "{\"status\": " << json_str(status)
    << ", \"displacement\": " << reals(vec_of(model->displacement));
  if (nls == NonLinearSolverType::_newton_raphson) {
    o << ", \"n_iter\": " << Int(solver.get("nb_iterations"))
      << ", \"error\": " << reals({Real(solver.get("error"))});
  } else if (nls == NonLinearSolverType::_petsc_snes) {
    o << ", \"n_iter\": " << Int(solver.get("n_iter"))
      << ", \"convergence_reason\": "
      << json_str(std::string(solver.get("convergence_reason")))
      << ", \"function_norm\": " << reals({Real(solver.get("function_norm"))})
      << ", \"solution_norm\": " << reals({Real(solver.get("solution_norm"))})
      << ", \"update_norm\": " << reals({Real(solver.get("update_norm"))})
      << ", \"error\": " << reals({Real(solver.get("error"))});
  }
  o << "}";
  return o.str();
}

} // namespace

/// OpenBLAS's core name and thread count (null / -1 if the BLAS is not
/// OpenBLAS), looked up in the global scope.
std::string blas_core() {
  using F = char * (*)();
  auto f = reinterpret_cast<F>(dlsym(RTLD_DEFAULT, "openblas_get_corename"));
  const char * c = f != nullptr ? f() : nullptr;
  return c != nullptr ? json_str(c) : "null";
}

int blas_threads() {
  using F = int (*)();
  auto f = reinterpret_cast<F>(dlsym(RTLD_DEFAULT, "openblas_get_num_threads"));
  return f != nullptr ? f() : -1;
}

int main(int argc, char * argv[]) {
  // D-050: host-independent dense blocking (Eigen's own defaults).
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  initialize(argc, argv);
  using P = std::vector<std::pair<std::string, std::string>>;
  const P direct{{"ksp_type", "preonly"}, {"pc_type", "lu"}};
  P snes = direct;
  for (auto kv : P{{"snes_rtol", "1e-12"},
                   {"snes_atol", "1e-14"},
                   {"snes_stol", "0"},
                   {"snes_max_it", "50"}}) {
    snes.push_back(kv);
  }
  P tao = direct;
  for (auto kv : P{{"tao_gatol", "1e-12"}, {"tao_grtol", "0"}}) {
    tao.push_back(kv);
  }
  std::ostringstream o;
  o << "{\n \"probe\": \"petsc_probe\",\n \"probe_version\": \"2\",\n";
  o << " \"host\": {\"eigen_cache_sizes\": [" << Eigen::l1CacheSize() << ", "
    << Eigen::l2CacheSize() << ", " << Eigen::l3CacheSize() << "], \"openblas_core\": "
    << blas_core() << ", \"openblas_threads\": " << blas_threads()
    << ", \"petsc_version\": [" << PETSC_VERSION_MAJOR << ", " << PETSC_VERSION_MINOR
    << ", " << PETSC_VERSION_SUBMINOR << "]},\n";
  o << " \"diag\": " << case_diag() << ",\n";
  o << " \"static_default\": " << case_static_default() << ",\n";
  o << " \"dynamic_default\": " << case_dynamic_default() << ",\n";
  o << " \"bar17_nr_cubic_default\": "
    << case_bar17(true, NonLinearSolverType::_newton_raphson, {}, false) << ",\n";
  o << " \"bar17_nr_cubic_direct\": "
    << case_bar17(true, NonLinearSolverType::_newton_raphson, {}, true) << ",\n";
  o << " \"bar17_snes_linear_direct\": "
    << case_bar17(false, NonLinearSolverType::_petsc_snes, snes, false) << ",\n";
  o << " \"bar17_snes_cubic_direct\": "
    << case_bar17(true, NonLinearSolverType::_petsc_snes, snes, false) << ",\n";
  o << " \"bar17_tao_linear_direct\": "
    << case_bar17(false, NonLinearSolverType::_petsc_tao, tao, false) << "\n}\n";
  std::cout << o.str();
  finalize();
  return 0;
}
