// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Adversarial oracle
// probe of the model couplers, not a port: runs the upstream model couplers
// (src/model/model_couplers: CouplerSolidCohesiveContact,
// CouplerSolidPhaseField, CouplerSolidContact) in configurations the couplers
// probe (oracle/probes/couplers/couplers_probe.cc) does not cover and prints
// plain-text records (doubles as IEEE-754 bit patterns in hex) for the Rust
// side (crates/akantu-couplers/tests/v80_review.rs, which reads the first run
// of each mode from crates/akantu-couplers/tests/data/v80_oracle/).
//
// Usage: v80_probe <mode> <upstream_src_root> <frozen_meshes_root>
//   cohesive_static     CouplerSolidCohesiveContact, _static, intrinsic
//                       cohesive_linear (zero strength, UD-224) on the
//                       cohesive-contact example mesh, compression: Newton
//                       history and the composition of K (coupler DOF
//                       manager vs solid alone vs the contact model's own
//                       DOF manager)
//   pf_selector_mismatch CouplerSolidPhaseField, material_multiple.dat, solid
//                       default selector (one material on both elements),
//                       phase field by physical names (two laws): strain
//                       transfer through different filters, 60 static steps
//   pf_initfull         CouplerSolidPhaseField::initFull (UD-617)
//   coupler_section     CouplerSolidContact with a `model
//                       coupler_solid_contact [ time_step_solver ... ]`
//                       section (UD-621); expected to crash
// Eigen sparse solver pinned for static solves (D-035); Eigen cache sizes
// pinned at the start of main (D-050).

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <contact_mechanics_model.hh>
#include <coupler_solid_cohesive_contact.hh>
#include <coupler_solid_contact.hh>
#include <coupler_solid_phasefield.hh>
#include <dof_manager.hh>
#include <material.hh>
#include <non_linear_solver.hh>
#include <phase_field_model.hh>
#include <phasefield.hh>
#include <solid_mechanics_model.hh>
#include <solid_mechanics_model_cohesive.hh>
#include <sparse_matrix_aij.hh>
#include <surface_selector.hh>

#include <Eigen/Core>

#include <bit>
#include <cstdint>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace fs = std::filesystem;

namespace {

fs::path g_up, g_frozen;

std::string hx(double v) {
  char buf[32];
  std::snprintf(buf, sizeof buf, "%016llx",
                static_cast<unsigned long long>(std::bit_cast<std::uint64_t>(v)));
  return buf;
}

void rec(const std::string & key, const std::vector<double> & v) {
  std::cout << "R " << key << " " << v.size();
  for (auto x : v) {
    std::cout << " " << hx(x);
  }
  std::cout << "\n";
}

void rec_arr(const std::string & key, const Array<Real> & a) {
  rec(key, std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent()));
}

void info(const std::string & key, const std::string & v) {
  std::cout << "I " << key << " " << v << "\n";
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

/// (nnz, sum, sum |.|) of a DOF manager's matrix K.
std::vector<double> k_sums(DOFManager & dm) {
  if (not dm.hasMatrix("K")) {
    return {-1., 0., 0.};
  }
  auto & k = aka::as_type<SparseMatrixAIJ>(dm.getMatrix("K"));
  const auto & a = k.getA();
  double s = 0, sa = 0;
  for (Int i = 0; i < k.getNbNonZero(); ++i) {
    s += a(i);
    sa += std::abs(a(i));
  }
  return {double(k.getNbNonZero()), s, sa};
}

/* ------------------------------------------------------------------------ */
void cohesive_static() {
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  std::ifstream f(g_up / (ex + "material-cohesive.dat"));
  std::stringstream ss;
  ss << f.rdbuf();
  auto text = ss.str();
  auto pos = text.find("epsilon_n = 0");
  if (pos == std::string::npos) {
    throw std::runtime_error("no epsilon_n = 0");
  }
  text.replace(pos, std::string("epsilon_n = 0").size(), "epsilon_n = 1e+03");
  auto dat = fs::temp_directory_path() / "v80_cohesive_static.dat";
  {
    std::ofstream o(dat, std::ios::binary | std::ios::trunc);
    o << text;
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  mesh.read((g_frozen / (ex + "cohesive-contact.msh")).string());
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
  info("cohesive_elements", std::to_string(mesh.getNbElement(_cohesive_2d_4)));
  info("contact_dof_manager_is_coupler_dof_manager",
       (&contact.getDOFManager() == &coupler.getDOFManager()) ? "yes" : "no");
  const Real inc = 1e-3 / 50.;
  for (Int s = 0; s < 10; ++s) {
    coupler.applyBC(BC::Dirichlet::IncrementValue(-inc, _y), "loading");
    coupler.applyBC(BC::Dirichlet::IncrementValue(inc, _y), "fixed");
    std::string status = "ok";
    try {
      coupler.solveStep();
    } catch (debug::NLSNotConvergedException & e) {
      status = "not_converged";
    }
    Real pen = 0;
    for (auto g : contact.getGaps()) {
      pen = std::max(pen, g);
    }
    info("step_" + std::to_string(s),
         status + " it " + std::to_string(Int(nls.get("nb_iterations"))) + " contact_elements " +
             std::to_string(contact.getContactElements().size()));
    rec("step_" + std::to_string(s) + "/error", {Real(nls.get("error"))});
    rec("step_" + std::to_string(s) + "/max_penetration", {pen});
    rec_arr("step_" + std::to_string(s) + "/displacement", solid.getDisplacement());
    if (status != "ok") {
      break;
    }
  }
  // the coupler's K: solid (need_to_reassemble = true), then the contact
  // stiffness for static / implicit dynamic
  coupler.assembleStiffnessMatrix();
  rec("K/coupler_after_coupler_assembly", k_sums(coupler.getDOFManager()));
  rec("K/contact_dof_manager_after_coupler_assembly", k_sums(contact.getDOFManager()));
  solid.assembleStiffnessMatrix(true);
  rec("K/coupler_after_solid_only_assembly", k_sums(coupler.getDOFManager()));
  rec_arr("contact_force", contact.getInternalForce());
  fs::remove(dat);
}

/// Triplets of a DOF manager's matrix K: irn, jcn (0-based, as doubles), a.
void k_triplets(const std::string & key, DOFManager & dm) {
  auto & k = aka::as_type<SparseMatrixAIJ>(dm.getMatrix("K"));
  std::vector<double> irn, jcn, a;
  for (Int i = 0; i < k.getNbNonZero(); ++i) {
    irn.push_back(double(k.getIRN()(i) - 1));
    jcn.push_back(double(k.getJCN()(i) - 1));
    a.push_back(k.getA()(i));
  }
  rec(key + "/irn", irn);
  rec(key + "/jcn", jcn);
  rec(key + "/a", a);
}

/// The cohesive_static setup, first increment applied, no solve: the
/// coupler's K (upstream: solid only) as triplets, then after one solve.
void cohesive_static_k0() {
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  std::ifstream f(g_up / (ex + "material-cohesive.dat"));
  std::stringstream ss;
  ss << f.rdbuf();
  auto text = ss.str();
  auto pos = text.find("epsilon_n = 0");
  text.replace(pos, std::string("epsilon_n = 0").size(), "epsilon_n = 1e+03");
  auto dat = fs::temp_directory_path() / "v80_cohesive_static_k0.dat";
  {
    std::ofstream o(dat, std::ios::binary | std::ios::trunc);
    o << text;
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  mesh.read((g_frozen / (ex + "cohesive-contact.msh")).string());
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
  coupler.getNonLinearSolver().set("max_iterations", 100);
  const Real inc = 1e-3 / 50.;
  coupler.applyBC(BC::Dirichlet::IncrementValue(-inc, _y), "loading");
  coupler.applyBC(BC::Dirichlet::IncrementValue(inc, _y), "fixed");
  coupler.assembleStiffnessMatrix();
  k_triplets("K0", coupler.getDOFManager());
  rec_arr("K0/displacement", solid.getDisplacement());
  coupler.solveStep();
  coupler.assembleStiffnessMatrix();
  k_triplets("K1", coupler.getDOFManager());
  rec_arr("K1/displacement", solid.getDisplacement());
  info("K1_contact_elements", std::to_string(contact.getContactElements().size()));
  fs::remove(dat);
}

/// The cohesive_static setup without the coupler and without contact:
/// SolidMechanicsModelCohesive alone (static, intrinsic, zero-strength
/// cohesive_linear), 4 compression increments.
void cohesive_static_solo() {
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  getStaticParser().parse((g_up / (ex + "material-cohesive.dat")).string());
  Mesh mesh(2);
  mesh.read((g_frozen / (ex + "cohesive-contact.msh")).string());
  SolidMechanicsModelCohesive solid(mesh);
  auto material_selector = std::make_shared<MeshDataMaterialCohesiveSelector>(solid);
  material_selector->setFallback(solid.getMaterialSelector());
  solid.setMaterialSelector(material_selector);
  solid.initFull(_analysis_method = _static, _is_extrinsic = false,
                 _solver_options = eigen_options());
  solid.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "sides");
  auto & nls = solid.getNonLinearSolver();
  nls.set("max_iterations", 100);
  const Real inc = 1e-3 / 50.;
  for (Int s = 0; s < 4; ++s) {
    solid.applyBC(BC::Dirichlet::IncrementValue(-inc, _y), "loading");
    solid.applyBC(BC::Dirichlet::IncrementValue(inc, _y), "fixed");
    std::string status = "ok";
    try {
      solid.solveStep();
    } catch (debug::NLSNotConvergedException & e) {
      status = "not_converged";
    }
    info("step_" + std::to_string(s), status + " it " + std::to_string(Int(nls.get("nb_iterations"))));
    rec("step_" + std::to_string(s) + "/error", {Real(nls.get("error"))});
    rec_arr("step_" + std::to_string(s) + "/displacement", solid.getDisplacement());
    if (status != "ok") {
      break;
    }
  }
}

/// Energy accounting under CouplerSolidContact (static Hertz, as the couplers
/// probe's contact_static_hertz, 5 steps): the solid's accumulated
/// "external work new" (only SolidMechanicsModel::assembleResidual adds to
/// it) and the on-the-fly "external work", "potential".
void coupler_energies() {
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  getStaticParser().parse((g_up / (ex + "material.dat")).string());
  Mesh mesh(2);
  mesh.read((g_frozen / (ex + "hertz.msh")).string());
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
  for (Int s = 0; s < 12; ++s) {
    coupler.applyBC(BC::Dirichlet::IncrementValue(-1e-4, _y), "loading");
    coupler.solveStep();
    rec("step_" + std::to_string(s) + "/energies",
        {solid.getEnergy("external work new"), solid.getEnergy("external work"),
         solid.getEnergy("potential"), double(contact.getContactElements().size())});
  }
}

/// The cohesive_static first increment with max_iterations = k (k = 1..4),
/// each on a fresh coupler: displacement, contact force, gaps after the
/// (failed) solve, to locate the first Newton iteration that differs.
void cohesive_static_iters() {
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  std::ifstream f(g_up / (ex + "material-cohesive.dat"));
  std::stringstream ss;
  ss << f.rdbuf();
  auto text = ss.str();
  auto pos = text.find("epsilon_n = 0");
  text.replace(pos, std::string("epsilon_n = 0").size(), "epsilon_n = 1e+03");
  auto dat = fs::temp_directory_path() / "v80_cohesive_static_iters.dat";
  {
    std::ofstream o(dat, std::ios::binary | std::ios::trunc);
    o << text;
  }
  getStaticParser().parse(dat.string());
  for (Int k = 1; k <= 4; ++k) {
    Mesh mesh(2);
    mesh.read((g_frozen / (ex + "cohesive-contact.msh")).string());
    CouplerSolidCohesiveContact coupler(mesh, _all_dimensions, "coupler_solid_contact");
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
    coupler.getNonLinearSolver().set("max_iterations", k);
    const Real inc = 1e-3 / 50.;
    coupler.applyBC(BC::Dirichlet::IncrementValue(-inc, _y), "loading");
    coupler.applyBC(BC::Dirichlet::IncrementValue(inc, _y), "fixed");
    try {
      coupler.solveStep();
    } catch (debug::NLSNotConvergedException & e) {
    }
    auto pre = "k" + std::to_string(k);
    rec(pre + "/error", {Real(coupler.getNonLinearSolver().get("error"))});
    rec_arr(pre + "/displacement", solid.getDisplacement());
    rec_arr(pre + "/contact_force", contact.getInternalForce());
    rec_arr(pre + "/gaps", contact.getGaps());
    rec_arr(pre + "/normals", contact.getNormals());
    rec_arr(pre + "/areas", contact.getNodalArea());
    rec_arr(pre + "/solid_internal_force", solid.getInternalForce());
    info(pre + "_contact_elements", std::to_string(contact.getContactElements().size()));
  }
  fs::remove(dat);
}

/* ------------------------------------------------------------------------ */
std::vector<double> law_internal(PhaseFieldModel & phase, Int l, const std::string & name) {
  auto & pf = phase.getPhaseField(l);
  const auto & cpf = static_cast<const PhaseField &>(pf);
  auto & fi = pf.getInternal<Real>(name);
  std::vector<double> out;
  for (auto type : cpf.getElementFilter().elementTypes(2, _not_ghost)) {
    const auto & a = fi(type, _not_ghost);
    out.insert(out.end(), a.data(), a.data() + a.size() * a.getNbComponent());
  }
  return out;
}

std::vector<double> mat_internal(SolidMechanicsModel & solid, Int i, const std::string & name) {
  auto & m = solid.getMaterial(i);
  const auto & cm = static_cast<const Material &>(m);
  auto & fi = m.getInternal<Real>(name);
  std::vector<double> out;
  for (auto type : cm.getElementFilter().elementTypes(2, _not_ghost)) {
    auto n = cm.getElementFilter(type, _not_ghost).size() *
             solid.getFEEngine().getNbIntegrationPoints(type) * fi(type, _not_ghost).getNbComponent();
    const auto & a = fi(type, _not_ghost);
    n = std::min<Int>(n, a.size() * a.getNbComponent());
    out.insert(out.end(), a.data(), a.data() + n);
  }
  return out;
}

void pf_selector_mismatch() {
  const auto dir = g_up / "test/test_model/test_phase_field_model";
  getStaticParser().parse((dir / "material_multiple.dat").string());
  Mesh mesh(2);
  mesh.read((dir / "test_two_element.msh").string());
  CouplerSolidPhaseField coupler(mesh);
  auto & solid = coupler.getSolidMechanicsModel();
  auto & phase = coupler.getPhaseFieldModel();
  // solid: default selector (every element gets material 0, "plate")
  solid.initFull(_analysis_method = _static, _solver_options = eigen_options());
  phase.setPhaseFieldSelector(
      std::make_shared<MeshDataPhaseFieldSelector<std::string>>("physical_names", phase));
  phase.initFull(_analysis_method = _static, _solver_options = eigen_options());
  info("nb_materials", std::to_string(solid.getNbMaterials()));
  info("nb_phase_fields", std::to_string(phase.getNbPhaseFields()));
  auto & u = solid.getDisplacement();
  auto & b = solid.getBlockedDOFs();
  const auto & X = mesh.getNodes();
  for (Int s = 0; s < 60; ++s) {
    Real eps = 2e-4 * Real(s);
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      if (X(n, 1) == -1.) {
        u(n, 0) = 0;
        u(n, 1) = 0;
        b(n, 0) = true;
        b(n, 1) = true;
      } else if (X(n, 1) == 1.) {
        u(n, 0) = 0;
        u(n, 1) = (X(n, 0) == 0.) ? 0.25 * eps : eps;
        b(n, 0) = true;
        b(n, 1) = true;
      }
    }
    std::string status = "ok";
    try {
      coupler.solve();
    } catch (debug::NLSNotConvergedException & e) {
      status = "not_converged";
    }
    auto pre = "step_" + std::to_string(s);
    info(pre, status + " solid_it " + std::to_string(Int(solid.getNonLinearSolver().get("nb_iterations"))) +
                  " phase_it " + std::to_string(Int(phase.getNonLinearSolver().get("nb_iterations"))));
    rec_arr(pre + "/displacement", u);
    rec_arr(pre + "/damage", phase.getDamage());
    for (Int l = 0; l < phase.getNbPhaseFields(); ++l) {
      rec(pre + "/law/" + std::to_string(l) + "/strain", law_internal(phase, l, "strain"));
    }
    rec(pre + "/material/0/damage", mat_internal(solid, 0, "damage"));
    rec(pre + "/material/0/stress", mat_internal(solid, 0, "stress"));
    if (status != "ok") {
      break;
    }
  }
}

/* ------------------------------------------------------------------------ */
void pf_initfull() {
  const auto dir = g_up / "test/test_model/test_phase_field_model";
  getStaticParser().parse((dir / "material_coupling.dat").string());
  Mesh mesh(2);
  mesh.read((dir / "test_one_element.msh").string());
  CouplerSolidPhaseField coupler(mesh);
  try {
    coupler.initFull(_analysis_method = _static);
    info("pf_initfull", "returned");
  } catch (debug::Exception & e) {
    info("pf_initfull_exception", e.what());
  } catch (std::exception & e) {
    info("pf_initfull_std_exception", e.what());
  }
}

/* ------------------------------------------------------------------------ */
void coupler_section() {
  const std::string ex = "examples/c++/contact_mechanics_model/hertz/";
  std::ifstream f(g_up / (ex + "material.dat"));
  std::stringstream ss;
  ss << f.rdbuf();
  auto text = ss.str() +
              "\nmodel coupler_solid_contact [\n"
              "  time_step_solver static [\n"
              "    name = static\n"
              "  ]\n"
              "]\n";
  auto dat = fs::temp_directory_path() / "v80_coupler_section.dat";
  {
    std::ofstream o(dat, std::ios::binary | std::ios::trunc);
    o << text;
  }
  getStaticParser().parse(dat.string());
  Mesh mesh(2);
  mesh.read((g_frozen / (ex + "hertz.msh")).string());
  std::cout << "I coupler_section constructing" << std::endl;
  try {
    CouplerSolidContact coupler(mesh);
    std::cout << "I coupler_section constructed" << std::endl;
  } catch (std::exception & e) {
    std::cout << "I coupler_section_exception " << e.what() << std::endl;
  }
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: v80_probe <mode> <upstream_src_root> <frozen_meshes_root>\n";
    return 2;
  }
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  std::string mode = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  std::cout.precision(17);
  try {
    if (mode == "cohesive_static") {
      cohesive_static();
    } else if (mode == "cohesive_static_iters") {
      cohesive_static_iters();
    } else if (mode == "coupler_energies") {
      coupler_energies();
    } else if (mode == "cohesive_static_solo") {
      cohesive_static_solo();
    } else if (mode == "cohesive_static_k0") {
      cohesive_static_k0();
    } else if (mode == "pf_selector_mismatch") {
      pf_selector_mismatch();
    } else if (mode == "pf_initfull") {
      pf_initfull();
    } else if (mode == "coupler_section") {
      coupler_section();
    } else {
      std::cerr << "unknown mode " << mode << "\n";
      return 2;
    }
  } catch (std::exception & e) {
    std::cout << "I uncaught_exception " << e.what() << std::endl;
    return 1;
  }
  std::cout.flush();
  finalize();
  return 0;
}
