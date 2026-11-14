// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the upstream PhaseFieldModel on the PETSc DOF
// manager with the bound-constrained NonLinearSolverTAO (GPCG) as
// test/test_model/test_phase_field_model/test_phase_field_optimal_linear.cc
// (TEST-095) and the phase-field half of test_phase_field_tao.cc
// (TEST-100) do, and prints one JSON document per case; every double is
// recorded as its IEEE-754 bit pattern (16 hex digits) next to a %.17g
// rendering, so the Rust side compares bits, never parsed decimals.
//
// Cases (argv[1]; one process per case, so that PETSc's global options of
// one case cannot reach another):
//   optimal_linear          TEST-095 as upstream: material_optimal_linear.dat
//                           (AT1, l0 = 20, irreversibility_type = tao),
//                           test_optimal_damage.msh, initDOFManager("petsc"),
//                           _petsc_tao + _petsc, tao_type gpcg, tao_gatol
//                           1e-9 (private options), bounds [0, 1] on the
//                           increment, d = 1 on the group `blocked`, one
//                           static step; the nodal damage and upstream's
//                           check (|d - (1 - |x|/(2 l0))^2| <= 1e-5). The
//                           test's `tao_monitor stdout` is not set (it would
//                           print into the JSON; no numerical effect).
//   tao_<law>_<split>       TEST-100's phase-field half on test_one_element.msh
//                           (physical-name selector): -tao_type gpcg and
//                           -tao_gatol 1e-9 in PETSc's GLOBAL database before
//                           initDOFManager("petsc") (as the test), bounds
//                           lower 0 / upper 1 - d rebuilt after every step with
//                           assembleToGlobalArray("damage", d, upper, -1.)
//                           (as the test), 1000 steps of eps_yy = 1e-5 s
//                           (s < 500) then (1000 - s) 1e-5, the strain
//                           [[0, 0], [0, eps_yy]] written into the law's
//                           `strain` internal at every quadrature point (the
//                           solid half of the test produces it; the coupler's
//                           role), phase.solveStep(); per step the nodal
//                           damage and getEnergy("dissipated"). law in
//                           {linear, quadratic}, split in {no_split,
//                           volumetric_deviatoric}; tao_linear_no_split reads
//                           upstream's material_linear_tao.dat unchanged, the
//                           others the same parameters with that law / split
//                           (recovery_tol only for linear).
//   cpl_tao_<law>_<split> (probe version 2) TEST-100 itself
//                           through CouplerSolidPhaseField: the same phase
//                           field as tao_<law>_<split> with the solid half of
//                           the test (`material phasefield <split>`, name
//                           plate, rho 1, E 210, nu 0.3, plane strain;
//                           cpl_tao_linear_no_split reads upstream's
//                           material_linear_tao.dat unchanged), the solid
//                           static with the Eigen sparse solver pinned
//                           (D-035; the test leaves the default solver) and
//                           its Newton at 100 iterations / 1e-9 (residual)
//                           as the test, the test's applyDisplacement (every
//                           node blocked, u_y = 0 at y = -0.5, u_y = eps_yy
//                           elsewhere), the test's 1000 steps, then
//                           coupler.solve("static", "") (the test's local
//                           strain / damage helpers are the coupler's
//                           transfers) and the bounds [0, 1 - d] as the
//                           test (the material file's text is recorded).
//                           Per step: eps_yy, the nodal damage, the
//                           dissipated and potential energies, the solid
//                           Newton count and error, the test's check values
//                           stress(0, 3) and the material damage(0), and
//                           SHA-256 digests (little-endian doubles) of the
//                           material stress and damage and the law's strain
//                           and phi (every displacement is prescribed); the
//                           full arrays after the last step.
//
// Usage: pftao_probe <case> <reference source tree>
//
// Host independence (D-050): Eigen's cache sizes are pinned to Eigen's own
// defaults at the start of main; the OpenBLAS core type / thread count come
// from the environment of the generator (OPENBLAS_CORETYPE=Haswell,
// OPENBLAS_NUM_THREADS=1 on x86_64); the record reports what Eigen and
// OpenBLAS use and the PETSc version.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <coupler_solid_phasefield.hh>
#include <dof_manager_petsc.hh>
#include <material.hh>
#include <mesh.hh>
#include <non_linear_solver.hh>
#include <non_linear_solver_tao.hh>
#include <phase_field_model.hh>
#include <phasefield.hh>
#include <solid_mechanics_model.hh>
#include <solver_vector_petsc.hh>

#include <Eigen/Core>
#include <dlfcn.h>
#include <petscoptions.h>
#include <petscversion.h>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <fstream>
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

ModelSolverOptions tao_options() {
  return ModelSolverOptions{.non_linear_solver_type = NonLinearSolverType::_petsc_tao,
                            .sparse_solver_type = SparseSolverType::_petsc};
}

/// TEST-095 (see the header).
std::string case_optimal_linear(const std::string & dir) {
  getStaticParser().parse(dir + "/material_optimal_linear.dat");
  Mesh mesh(2);
  mesh.read(dir + "/test_optimal_damage.msh");
  PhaseFieldModel phase(mesh);
  phase.initDOFManager("petsc");
  phase.initFull(_analysis_method = _static, _solver_options = tao_options());
  auto & tao_solver = aka::as_type<NonLinearSolverTAO>(phase.getNonLinearSolver());
  tao_solver.set("tao_type", "gpcg");
  tao_solver.set("tao_gatol", 1e-9);
  auto & dof_manager = aka::as_type<DOFManagerPETSc>(phase.getDOFManager());
  SolverVectorPETSc lower_bound(dof_manager, "lower_bound");
  SolverVectorPETSc upper_bound(dof_manager, "upper_bound");
  lower_bound.resize();
  upper_bound.resize();
  lower_bound.set(0.);
  upper_bound.set(1.);
  tao_solver.setBounds(lower_bound, upper_bound);
  auto & phasefield = phase.getPhaseField(0);
  const Real l0 = phasefield.getParam("l0");
  phase.applyBC(BC::Dirichlet::FixedValue(1., _x), "blocked");
  std::string status = "ok";
  try {
    phase.solveStep("static");
    phase.savePreviousState();
  } catch (std::exception & e) {
    status = e.what();
  }
  auto & damage = phase.getDamage();
  auto & positions = phase.getMesh().getNodes();
  Real max_error = 0.;
  Int failing = -1;
  for (Int n = 0; n < phase.getMesh().getNbNodes(); ++n) {
    Real x = positions(n, 0);
    Real optimal = std::pow((1. - std::abs(x) / (2. * l0)), 2.) * (x > -2. * l0) * (x < 2. * l0);
    Real e = std::abs(optimal - damage(n));
    max_error = std::max(max_error, e);
    if (e > 1e-5 && failing < 0) {
      failing = n;
    }
  }
  std::ostringstream o;
  o << "{\"status\": " << json_str(status) << ", \"nb_nodes\": " << phase.getMesh().getNbNodes()
    << ", \"damage\": " << reals(vec_of(damage))
    << ", \"upstream_check\": {\"max_error\": " << reals({max_error})
    << ", \"first_failing_node\": " << failing << "}}";
  return o.str();
}

std::string material_text(const std::string & law, const std::string & split) {
  std::ostringstream m;
  m << "model phase_field_model [\n  phasefield " << law << " " << split
    << " [\n    name = plate\n    E = 210.0\n    nu = 0.3\n    gc = 5e-3\n    l0 = 1.\n"
    << "    Plane_Stress = false\n    irreversibility_type = tao\n"
    << "    irreversibility_tol = 0.001\n";
  if (law == "linear") {
    m << "    recovery_tol = 0.001\n";
  }
  m << "  ]\n]\n";
  return m.str();
}

/// TEST-100's phase-field half (see the header).
std::string case_tao(const std::string & dir, const std::string & law,
                     const std::string & split) {
  std::string material;
  if (law == "linear" && split == "no_split") {
    material = dir + "/material_linear_tao.dat";
  } else {
    material = "/tmp/pftao_" + law + "_" + split + ".dat";
    std::ofstream f(material);
    f << material_text(law, split);
  }
  getStaticParser().parse(material);
  const Int dim = 2;
  Mesh mesh(dim);
  mesh.read(dir + "/test_one_element.msh");
  PhaseFieldModel phase(mesh);
  auto && selector =
      std::make_shared<MeshDataPhaseFieldSelector<std::string>>("physical_names", phase);
  phase.setPhaseFieldSelector(selector);
  PetscOptionsSetValue(NULL, "-tao_type", "gpcg");
  PetscOptionsSetValue(NULL, "-tao_gatol", "1e-9");
  phase.initDOFManager("petsc");
  auto & dof_manager = aka::as_type<DOFManagerPETSc>(phase.getDOFManager());
  phase.initFull(_analysis_method = _static, _solver_options = tao_options());
  auto & tao_solver = aka::as_type<NonLinearSolverTAO>(phase.getNonLinearSolver());
  SolverVectorPETSc lower_bound(dof_manager, "lower_bound");
  lower_bound.resize();
  lower_bound.set(0.);
  SolverVectorPETSc upper_bound(dof_manager, "upper_bound");
  upper_bound.resize();
  upper_bound.set(1.);
  tao_solver.setBounds(lower_bound, upper_bound);
  auto & dof_damage = dof_manager.getDOFs("damage");
  auto & pf = phase.getPhaseField(0);
  auto & strain = pf.getInternal<Real>("strain");
  const Int steps = 1000;
  const Real increment = 1e-5;
  std::vector<double> strains, damages, energies;
  std::string status = "ok";
  Int done = 0;
  for (Int s = 0; s < steps; ++s) {
    Real axial_strain = s < 500 ? increment * s : (1000 - double(s)) * increment;
    for (auto && eps : make_view(strain(_quadrangle_4, _not_ghost), dim, dim)) {
      eps.zero();
      eps(1, 1) = axial_strain;
    }
    try {
      phase.solveStep();
    } catch (std::exception & e) {
      status = "step " + std::to_string(s) + ": " + e.what();
      break;
    }
    lower_bound.set(0.);
    upper_bound.set(1.);
    dof_manager.assembleToGlobalArray("damage", dof_damage, upper_bound, -1.);
    tao_solver.setBounds(lower_bound, upper_bound);
    strains.push_back(axial_strain);
    for (auto d : vec_of(phase.getDamage())) {
      damages.push_back(d);
    }
    energies.push_back(phase.getEnergy("dissipated"));
    ++done;
  }
  std::ostringstream o;
  o << "{\"status\": " << json_str(status) << ", \"law\": " << json_str(law)
    << ", \"split\": " << json_str(split) << ", \"material\": " << json_str(material)
    << ", \"steps\": " << done << ", \"nb_nodes\": " << mesh.getNbNodes()
    << ", \"strain_yy\": " << reals(strains) << ", \"damage\": " << reals(damages)
    << ", \"energy\": " << reals(energies) << "}";
  return o.str();
}

/// SHA-256 (hex) of the little-endian doubles of `v` (as the couplers
/// probe's per-step digests).
std::string digest(const std::vector<double> & v) {
  akantu_probe::Sha256 h;
  h.update(v.data(), v.size() * sizeof(double));
  return h.hex();
}

std::string strings(const std::vector<std::string> & v) {
  std::ostringstream o;
  o << "[";
  for (std::size_t i = 0; i < v.size(); ++i) {
    o << (i ? ", " : "") << json_str(v[i]);
  }
  return o.str() + "]";
}

std::string ints(const std::vector<long long> & v) {
  std::ostringstream o;
  o << "[";
  for (std::size_t i = 0; i < v.size(); ++i) {
    o << (i ? ", " : "") << v[i];
  }
  return o.str() + "]";
}

/// The not-ghost quadrangle block of an internal of a constitutive law.
template <class Law>
std::vector<double> internal_of(Law & law, const std::string & name) {
  return vec_of(law.template getInternal<Real>(name)(_quadrangle_4, _not_ghost));
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

std::string coupled_material_text(const std::string & law, const std::string & split) {
  std::ostringstream m;
  m << "model solid_mechanics_model [\n  material phasefield " << split
    << " [\n    name = plate\n    rho = 1.\n    E = 210.0\n    nu = 0.3\n"
    << "    Plane_Stress = false\n  ]\n]\n\n"
    << material_text(law, split);
  return m.str();
}

/// TEST-100 through CouplerSolidPhaseField (see the header).
std::string case_coupled_tao(const std::string & dir, const std::string & law,
                             const std::string & split) {
  std::string material;
  if (law == "linear" && split == "no_split") {
    material = dir + "/material_linear_tao.dat";
  } else {
    material = "/tmp/pftao_cpl_" + law + "_" + split + ".dat";
    std::ofstream f(material);
    f << coupled_material_text(law, split);
  }
  getStaticParser().parse(material);
  std::string material_contents;
  {
    std::ifstream f(material);
    std::stringstream ss;
    ss << f.rdbuf();
    material_contents = ss.str();
  }
  const Int dim = 2;
  Mesh mesh(dim);
  mesh.read(dir + "/test_one_element.msh");
  CouplerSolidPhaseField coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModel();
  auto & phase = coupler.getPhaseFieldModel();
  solid.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto & solver = solid.getNonLinearSolver("static");
  solver.set("max_iterations", 100);
  solver.set("threshold", 1e-9);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  auto && selector =
      std::make_shared<MeshDataPhaseFieldSelector<std::string>>("physical_names", phase);
  phase.setPhaseFieldSelector(selector);
  PetscOptionsSetValue(NULL, "-tao_type", "gpcg");
  PetscOptionsSetValue(NULL, "-tao_gatol", "1e-9");
  phase.initDOFManager("petsc");
  auto & dof_manager = aka::as_type<DOFManagerPETSc>(phase.getDOFManager());
  phase.initFull(_analysis_method = _static, _solver_options = tao_options());
  auto & tao_solver = aka::as_type<NonLinearSolverTAO>(phase.getNonLinearSolver());
  SolverVectorPETSc lower_bound(dof_manager, "lower_bound");
  lower_bound.resize();
  lower_bound.set(0.);
  SolverVectorPETSc upper_bound(dof_manager, "upper_bound");
  upper_bound.resize();
  upper_bound.set(1.);
  tao_solver.setBounds(lower_bound, upper_bound);
  auto & dof_damage = dof_manager.getDOFs("damage");
  auto & mat = solid.getMaterial(0);
  auto & pf = phase.getPhaseField(0);
  const Int steps = 1000;
  const Real increment = 1e-5;
  std::vector<double> strains, damages, e_diss, e_pot, s_err, stress_yy, mat_damage0;
  std::vector<long long> s_it;
  std::vector<std::string> d_stress, d_mdam, d_lstrain, d_lphi;
  std::string status = "ok";
  Int done = 0;
  for (Int s = 0; s < steps; ++s) {
    Real axial_strain = s < 500 ? increment * s : (1000 - double(s)) * increment;
    // applyDisplacement of test_phase_field_tao.cc
    auto & u = solid.getDisplacement();
    auto & blocked = solid.getBlockedDOFs();
    const auto & X = mesh.getNodes();
    for (Int n = 0; n < mesh.getNbNodes(); ++n) {
      u(n, 0) = 0;
      u(n, 1) = X(n, 1) == -0.5 ? 0. : axial_strain;
      blocked(n, 0) = true;
      blocked(n, 1) = true;
    }
    try {
      coupler.solve("static", "");
    } catch (std::exception & e) {
      status = "step " + std::to_string(s) + ": " + e.what();
      break;
    }
    lower_bound.set(0.);
    upper_bound.set(1.);
    dof_manager.assembleToGlobalArray("damage", dof_damage, upper_bound, -1.);
    tao_solver.setBounds(lower_bound, upper_bound);
    strains.push_back(axial_strain);
    for (auto d : vec_of(phase.getDamage())) {
      damages.push_back(d);
    }
    e_diss.push_back(phase.getEnergy("dissipated"));
    e_pot.push_back(solid.getEnergy("potential"));
    s_it.push_back(Int(solver.get("nb_iterations")));
    s_err.push_back(Real(solver.get("error")));
    auto stress = internal_of(mat, "stress");
    auto mdam = internal_of(mat, "damage");
    stress_yy.push_back(stress[3]);
    mat_damage0.push_back(mdam[0]);
    d_stress.push_back(digest(stress));
    d_mdam.push_back(digest(mdam));
    d_lstrain.push_back(digest(internal_of(pf, "strain")));
    d_lphi.push_back(digest(internal_of(pf, "phi")));
    ++done;
  }
  std::ostringstream o;
  o << "{\"status\": " << json_str(status) << ", \"law\": " << json_str(law)
    << ", \"split\": " << json_str(split) << ", \"material\": " << json_str(material)
    << ", \"material_text\": " << json_str(material_contents)
    << ", \"steps\": " << done << ", \"nb_nodes\": " << mesh.getNbNodes()
    << ", \"strain_yy\": " << reals(strains) << ", \"damage\": " << reals(damages)
    << ", \"energy\": " << reals(e_diss) << ", \"potential_energy\": " << reals(e_pot)
    << ", \"solid_iterations\": " << ints(s_it) << ", \"solid_error\": " << reals(s_err)
    << ", \"stress_yy_q0\": " << reals(stress_yy)
    << ", \"material_damage_q0\": " << reals(mat_damage0)
    << ", \"digests\": {\"material_stress\": " << strings(d_stress)
    << ", \"material_damage\": " << strings(d_mdam)
    << ", \"law_strain\": " << strings(d_lstrain) << ", \"law_phi\": " << strings(d_lphi)
    << "}, \"final\": {\"displacement\": " << reals(vec_of(solid.getDisplacement()))
    << ", \"internal_force\": " << reals(vec_of(solid.getInternalForce()))
    << ", \"damage\": " << reals(vec_of(phase.getDamage()))
    << ", \"material_stress\": " << reals(internal_of(mat, "stress"))
    << ", \"material_grad_u\": " << reals(internal_of(mat, "grad_u"))
    << ", \"material_damage\": " << reals(internal_of(mat, "damage"))
    << ", \"law_damage\": " << reals(internal_of(pf, "damage"))
    << ", \"law_strain\": " << reals(internal_of(pf, "strain"))
    << ", \"law_phi\": " << reals(internal_of(pf, "phi")) << "}}";
  return o.str();
}

} // namespace

int main(int argc, char * argv[]) {
  // D-050: host-independent dense blocking (Eigen's own defaults).
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (argc < 3) {
    std::cerr << "usage: pftao_probe <case> <reference source tree>\n";
    return 2;
  }
  const std::string which = argv[1];
  const std::string dir = std::string(argv[2]) + "/test/test_model/test_phase_field_model";
  int ac = 1;
  initialize(ac, argv);
  std::string body;
  if (which == "optimal_linear") {
    body = case_optimal_linear(dir);
  } else {
    bool found = false;
    for (std::string law : {"linear", "quadratic"}) {
      for (std::string split : {"no_split", "volumetric_deviatoric"}) {
        if (which == "tao_" + law + "_" + split) {
          body = case_tao(dir, law, split);
          found = true;
        }
        if (which == "cpl_tao_" + law + "_" + split) {
          body = case_coupled_tao(dir, law, split);
          found = true;
        }
      }
    }
    if (not found) {
      std::cerr << "unknown case " << which << "\n";
      return 2;
    }
  }
  std::ostringstream o;
  // version 2: the cpl_tao_* cases; the version-1 cases
  // print the same result.
  o << "{\n \"probe\": \"pftao_probe\",\n \"probe_version\": \"2\",\n \"case\": "
    << json_str(which) << ",\n";
  o << " \"host\": {\"eigen_cache_sizes\": [" << Eigen::l1CacheSize() << ", "
    << Eigen::l2CacheSize() << ", " << Eigen::l3CacheSize() << "], \"openblas_core\": "
    << blas_core() << ", \"openblas_threads\": " << blas_threads()
    << ", \"petsc_version\": [" << PETSC_VERSION_MAJOR << ", " << PETSC_VERSION_MINOR << ", "
    << PETSC_VERSION_SUBMINOR << "]},\n";
  o << " \"result\": " << body << "\n}\n";
  std::cout << o.str();
  finalize();
  return 0;
}
