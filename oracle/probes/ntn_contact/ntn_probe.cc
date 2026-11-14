// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the classes of the extra package
// traction-at-split-node-contact (NTNContact, NTRFContact, MIIASYMContact,
// the friction laws x regularisations through initializeNTNFriction,
// NTNContactSolverCallback, ParameterReader, manual restart, SpringBC,
// ForceBasedDirichlet) and records the state for the Rust differential
// tests (crates/akantu-ntn-contact/tests/oracle_ntn_fixtures.rs,
// fixtures/model/ntn_contact/*). The package classes are the upstream ones,
// unmodified; this file only drives them.
//
// Cases (case ids model/ntn_contact/<case>, format v1):
//   basics_2d, basics_3d
//       TestNTNFixture::SetUp of test_ntn_fixture.hh (serial) + initDynamics(0.1):
//       slaves, masters, normals, lumped boundaries, impedances
//   compression
//       TestNTNFrictionFixture.TestCompression (linear_slip_weakening_no_healing,
//       option no_regularisation): every step contact pressure, friction
//       traction, strength, slips, sticking, contact flags, external work,
//       energies; then the restart files of the contact and the friction, a
//       fresh friction reading them back (UD-394), and the manual displacement
//       restart file
//   ras_<file>
//       ParamTestNTNFrictionFixture.TestRateAndState for each of the 15
//       material_ntn_friction_rate_and_state_*.dat files (the 12 upstream
//       instantiates and the 3 revised_n_shaped ones it comments out):
//       energies every step, contact/friction state every 10 steps
//   ras_n_shaped_slip_law, ras_legacy_n_shaped_slip
//       the same scenario on upstream's n_shaped_slip data with the friction
//       line replaced by rate_and_state:n_shaped:slip and by the legacy key
//       rate_and_state_n_shaped_slip (upstream's own n_shaped_slip file runs
//       n_shaped:ageing, UD-405)
//   law_<law>__<reg>
//       probe scenario on ntn_test_compression.msh: compression then a
//       tangential traction ramp (slider free in x), 150 steps, for 6 laws x
//       3 regularisation options, plus Dieterich at rest (UD-039) and the
//       simplified Prakash-Clifton regularisation with t_star = 0 (UD-397)
//   ntrf_coulomb
//       NTRFContact (rigid plane y = 0) + its NTRFFriction (coulomb) with a
//       copy of NTNContactSolverCallback::assembleResidual (probe-defined)
//   miiasym_fields
//       MIIASYMContact: impedance, relative fields, computeContactPressure
//       (no-op) and computeContactPressureInEquilibrium
//   factory_selection
//       printself of initializeNTNFriction for every registered law name x
//       {no_regularisation, rubin_ampuero, simplified_prakash_clifton, bogus}
//       (UD-039 selector evidence)
//   parameter_reader
//       ParameterReader::readInputFile + writeInputFile + get<T>
//   bc_spring
//       SpringBC then ForceBasedDirichlet driven explicit run
//   (InclinedFlatDirichlet is not probed: applyBC with it does not compile,
//   UD-403)
//
// Cases whose upstream run produces NaN carry a manifest note naming the
// quantities, the first step and the cause (D-046 (4), table kNanNotes).
// Eigen's cache sizes are pinned to its defaults at the start of main
// (D-050: Eigen::setCpuCacheSizes(16384, 524288, 524288)); the explicit
// lumped-mass runs of this probe solve no Eigen system, so the pin changes no
// recorded value; cases generated since the pin record it as
// parameters.eigen_cache_sizes.
//
// Usage: ntn_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root> <tmp_dir>
// Environment: NTN_PROBE_CASES=<id>[,<id>...] restricts the run to the listed
// case ids (without the model/ntn_contact/ prefix).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <communicator.hh>
#include <mesh.hh>
#include <solid_mechanics_model.hh>

#include <mIIasym_contact.hh>
#include <manual_restart.hh>
#include <ntn_base_contact.hh>
#include <ntn_base_friction.hh>
#include <ntn_contact.hh>
#include <ntn_contact_solvercallback.hh>
#include <ntn_initiation_function.hh>
#include <ntrf_contact.hh>
#include <parameter_reader.hh>
// boundary-condition helpers (header only; need the model and
// integrateResidual first)
#include <boundary_functions.hh>
#include <force_based_dirichlet.hh>
#include <inclined_flat_dirichlet.hh>
#include <spring_bc.hh>

#include <Eigen/Core>

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <memory>
#include <set>
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

const char * const kProbe = "ntn_probe";
const char * const kProbeVersion = "1";

fs::path g_root;
fs::path g_src;
fs::path g_frozen;
fs::path g_tmp;
std::set<std::string> g_only;

bool selected(const std::string & id) {
  return g_only.empty() or g_only.count(id) > 0;
}

fs::path test_dir() {
  return g_src / "extra_packages/traction-at-split-node-contact/test/test_ntn_model";
}

std::string upstream_test(const std::string & rel) {
  return "upstream:extra_packages/traction-at-split-node-contact/test/test_ntn_model/" + rel;
}

template <class T> std::vector<double> vec_of(const Array<T> & a) {
  std::vector<double> v;
  v.reserve(a.size() * a.getNbComponent());
  for (Int i = 0; i < a.size(); ++i) {
    for (Int c = 0; c < a.getNbComponent(); ++c) {
      v.push_back(static_cast<double>(a(i, c)));
    }
  }
  return v;
}

std::vector<std::int64_t> ivec_of(const Array<Idx> & a) {
  std::vector<std::int64_t> v;
  for (Int i = 0; i < a.size(); ++i) {
    v.push_back(a(i));
  }
  return v;
}

/// Per-step series: `name -> values` with a fixed inner shape.
struct Recorder {
  struct Series {
    std::vector<double> values;
    std::vector<std::size_t> inner;
    std::vector<std::string> axes;
    std::size_t rows{0};
  };
  std::map<std::string, Series> series;

  void push(const std::string & name, const std::vector<double> & v,
            const std::vector<std::size_t> & inner,
            const std::vector<std::string> & axes) {
    auto & s = series[name];
    if (s.rows == 0) {
      s.inner = inner;
      s.axes = axes;
    }
    s.values.insert(s.values.end(), v.begin(), v.end());
    s.rows += 1;
  }

  template <class T>
  void push_array(const std::string & name, const Array<T> & a) {
    push(name, vec_of(a),
         {static_cast<std::size_t>(a.size()),
          static_cast<std::size_t>(a.getNbComponent())},
         {"entry", "component"});
  }

  void write(ap::FixtureWriter & w, const std::string & row_axis) {
    for (auto & [name, s] : series) {
      std::vector<std::size_t> shape{s.rows};
      shape.insert(shape.end(), s.inner.begin(), s.inner.end());
      std::vector<std::string> axes{row_axis};
      axes.insert(axes.end(), s.axes.begin(), s.axes.end());
      w.add(name, shape, axes, "table", s.values);
    }
  }
};

/// The class chain of a friction `printself` (one `Class [` line per
/// NTN*/NTRF* class, outer to inner): the full text holds heap addresses
/// (`+ address`), which would break the determinism check, and is large.
std::string class_chain(const std::string & text) {
  std::istringstream is(text);
  std::string line, out;
  while (std::getline(is, line)) {
    auto p = line.find_first_not_of(" \t");
    if (p == std::string::npos) {
      continue;
    }
    auto t = line.substr(p);
    auto w = t.substr(0, t.find(' '));
    if ((w.rfind("NTN", 0) == 0 or w.rfind("NTRF", 0) == 0) and
        t.find('[') != std::string::npos) {
      out += w + " [\n";
    }
  }
  return out;
}

template <class T>
void add_array(ap::FixtureWriter & w, const std::string & name,
               const Array<T> & a) {
  w.add(name,
        {static_cast<std::size_t>(a.size()),
         static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a));
}

const std::string kMaterials = R"(
material elastic [
     name = slider
     rho  = 1200
     nu   = 0.33
     E    = 7.98e9
     Plane_Stress = 1
]

material elastic [
     name = base
     rho  = 1200
     nu   = 0.33
     E    = 7.98e9
     Plane_Stress = 1
]
)";

fs::path write_text(const std::string & name, const std::string & text) {
  auto p = g_tmp / name;
  std::ofstream f(p);
  f << text;
  return p;
}

// D-050: Eigen's own default cache sizes (host-independent blocking).
constexpr std::ptrdiff_t kEigenL1 = 16384, kEigenL2 = 524288, kEigenL3 = 524288;

void record_eigen_pin(ap::FixtureWriter & w) {
  ap::Json c = ap::Json::array();
  c.push(static_cast<long long>(kEigenL1));
  c.push(static_cast<long long>(kEigenL2));
  c.push(static_cast<long long>(kEigenL3));
  w.parameters()["eigen_cache_sizes"] = c;
}

// D-046 (4): every case whose upstream run produces NaN documents it (which
// quantities, from which step, why) in its manifest notes.
const std::map<std::string, std::string> kNanNotes = {
      {"law_coulomb__spc_tstar0",
       "NaN (D-046): frictional_strength holds the default quiet NaN of an invalid operation in all 1650 values (every contact node, every step from step 0). Cause UD-397: upstream's simplified Prakash-Clifton regularisation computes alpha = dt/t_star with its default t_star = 0, so alpha = inf and the strength (s + alpha*s_law)/(1 + alpha) = inf/inf is NaN; the nodes then count as sticking (the alpha test is false for NaN). Reproduced in upstream-compat mode; the port's corrected default uses the t_star -> 0 limit (the unregularised strength)."},
      {"law_dieterich_ageing_at_rest__none",
       "NaN (D-046): default quiet NaNs of invalid operations in frictional_strength (steps 0-9, 110 values: a -inf coefficient times a zero contact pressure), friction_traction (from step 10: the y component at step 10, both components from step 11; 3069 values), slip, cumulative_slip, slip_velocity_norm and friction_state (from step 11, 1529 values each), external_work (from step 11, 278 values) and displacement_every_10 (from sample 1 = step 10, 5788 values). Cause UD-039: the Dieterich coefficient mu_0 + a ln(V/v_star) + b ln(theta/phi_star) is -inf at rest (V = 0 on every sticking node), so the strength is -inf*0 = NaN while the pressure is zero and -inf at step 10; the friction traction is then infinite, the explicit Newmark corrector multiplies beta = 0 by an infinite acceleration and the run diverges. Reproduced in upstream-compat mode; the port's corrected default stops with the UD-039 domain error."},
      {"ras_revised_n_shaped_slip",
       "NaN (D-046): default quiet NaNs of invalid operations in sampled/friction_state and sampled/frictional_strength (985 of 1000 values each, from sample 1 = step 10). Cause UD-405: upstream comments this data file out of INSTANTIATE_TEST_SUITE_P; the revised N-shaped coefficient vanishes as V -> 0, so the file's fss is not a steady state, the slip velocity drops to zero, and the slip state law -(theta V/d_c) ln(theta V/d_c) dt evaluated on a non-sticking node with V = 0 is 0*(-inf) = NaN; the NaN state then gives a NaN strength. Reproduced in upstream-compat mode."},
      {"miiasym_fields",
       "NaN (D-046): pressure_after_noop holds 22 values 0x7ff8000000000000, std::numeric_limits<Real>::quiet_NaN() (a constant, not an invalid operation: the same bits on every platform): NTNBaseContact::addSplitNode initialises the contact pressure with quiet_NaN and MIIASYMContact::computeContactPressure is a no-op by design (upstream behaviour); computeContactPressureInEquilibrium then overwrites it (pressure_in_equilibrium)."},
};

void set_nan_notes(ap::FixtureWriter & w, const std::string & cid) {
  auto it = kNanNotes.find(cid);
  if (it != kNanNotes.end()) {
    w.set_notes(it->second);
  }
}

/// Friction/contact state of every contact node.
void record_contact_state(Recorder & r, NTNBaseContact & contact,
                          NTNBaseFriction & friction) {
  r.push_array("contact_pressure", contact.getContactPressure().getArray());
  r.push_array("is_in_contact", contact.getIsInContact().getArray());
  r.push_array("friction_traction", friction.getFrictionTraction().getArray());
  r.push_array("frictional_strength",
               friction.getFrictionalStrength().getArray());
  r.push_array("is_sticking", friction.getIsSticking().getArray());
  r.push_array("slip", friction.getSlip().getArray());
  r.push_array("cumulative_slip", friction.getCumulativeSlip().getArray());
  r.push_array("slip_velocity_norm", friction.getSlipVelocityNorm().getArray());
  r.push_array("friction_state", friction.getState().getArray());
}

/// Copy of NTNContactSolverCallback for an arbitrary contact (the upstream
/// class creates an NTNContact itself).
class ProbeCallback : public InterceptSolverCallback {
public:
  ProbeCallback(SolidMechanicsModel & solid, NTNBaseContact & contact,
                NTNBaseFriction & friction)
      : InterceptSolverCallback(solid), solid(solid), contact(contact),
        friction(friction) {}

  Vector<Real, 2> workIncrement() const {
    Vector<Real, 2> work{0., 0.};
    const auto & contact_pressure = contact.getGlobalContactPressure();
    const auto & friction_traction = friction.getGlobalFrictionTraction();
    const auto & mesh = solid.getMesh();
    auto spatial_dimension = mesh.getSpatialDimension();
    for (auto && [n, inc, f_cont, f_fric] :
         enumerate(make_view(solid.getIncrement(), spatial_dimension),
                   make_view(contact_pressure, spatial_dimension),
                   make_view(friction_traction, spatial_dimension))) {
      auto is_local_node = mesh.isLocalOrMasterNode(n);
      auto is_not_pbc_slave_node = not mesh.isPeriodicSlave(n);
      Real count_node = is_local_node && is_not_pbc_slave_node ? 1. : 0.;
      work[0] -= count_node * f_cont.dot(inc);
      work[1] -= count_node * f_fric.dot(inc);
    }
    return work;
  }

  void assembleResidual() override {
    auto && dof_manager = solid.getDOFManager();
    auto & contact_pressure = contact.getGlobalContactPressure();
    auto & friction_traction = friction.getGlobalFrictionTraction();
    if (external_work_release != solid.getDisplacementRelease()) {
      external_work += this->workIncrement();
      external_work_release = solid.getDisplacementRelease();
    }
    aka::as_type<SolverCallback>(solid).assembleResidual();
    contact.computeContactPressure();
    friction.updateSlip();
    friction.computeFrictionTraction();
    contact.assembleGlobalContactPressure();
    friction.assembleGlobalFrictionTraction();
    dof_manager.assembleToResidual("displacement", contact_pressure, 1);
    dof_manager.assembleToResidual("displacement", friction_traction, 1);
  }

  void beforeSolveStep() override {
    aka::as_type<SolverCallback>(solid).beforeSolveStep();
    friction.savePreviousState();
    contact.savePreviousState();
  }

  void afterSolveStep(bool converged = true) override {
    aka::as_type<SolverCallback>(solid).afterSolveStep(converged);
    if (not converged) {
      friction.restorePreviousState();
      contact.restorePreviousState();
    }
  }

  Vector<Real, 2> external_work{0., 0.};

private:
  SolidMechanicsModel & solid;
  NTNBaseContact & contact;
  NTNBaseFriction & friction;
  Int external_work_release{-1};
};

/* -------------------------------------------------------------------------- */
void case_basics(Int dim) {
  std::string cid = "basics_" + std::to_string(dim) + "d";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / ("ntn_test_" + std::to_string(dim) + "d.msh");
  auto mat_file = test_dir() / "data/material_ntn_model.dat";
  Mesh mesh(dim);
  mesh.read(mesh_file.string());
  SolidMechanicsModel model(mesh);
  auto contact = std::make_shared<NTNContact>(model);
  getStaticParser().parse(mat_file.string());
  model.initFull(_analysis_method = _explicit_lumped_mass);
  model.assembleMassLumped();
  contact->addSurfacePair("slider_bottom", "base_top", 1);
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "TestNTNFixture<" + std::to_string(dim) +
                          "> (test_ntn_basics.cc) setup and initDynamics(0.1)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  w.add_input_file("mesh", mesh_file, upstream_test(mesh_file.filename().string()));
  w.add_input_file("material", mat_file, upstream_test("data/material_ntn_model.dat"));
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  // impedance right after addSurfacePair (time step not set yet)
  w.add("impedance_before_time_step",
        {static_cast<std::size_t>(contact->getNbContactNodes())}, {"entry"},
        "table", vec_of(contact->getImpedance().getArray()));
  contact->updateNormals();
  contact->updateLumpedBoundary();
  Real stable = model.getStableTimeStep();
  model.setTimeStep(stable * 0.1);
  contact->updateImpedance();
  p["stable_time_step"] = stable;
  p["time_step"] = model.getTimeStep();
  // exact values (the JSON parameters are printed with 16 digits)
  w.add("stable_time_step", {1}, {"entry"}, "table", std::vector<double>{stable});
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{model.getTimeStep()});
  w.add("slaves", {static_cast<std::size_t>(contact->getNbContactNodes())},
        {"entry"}, "table", ivec_of(contact->getSlaves().getArray()));
  w.add("masters", {static_cast<std::size_t>(contact->getNbContactNodes())},
        {"entry"}, "table", ivec_of(contact->getMasters().getArray()));
  add_array(w, "normals", contact->getNormals().getArray());
  add_array(w, "lumped_boundary_slaves", contact->getLumpedBoundarySlaves().getArray());
  add_array(w, "lumped_boundary_masters", contact->getLumpedBoundaryMasters().getArray());
  add_array(w, "impedance", contact->getImpedance().getArray());
  add_array(w, "mass", model.getMass());
  w.finish();
}

/* -------------------------------------------------------------------------- */
void apply_fixed_bcs(SolidMechanicsModel & model, bool block_slider_x) {
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_right");
  if (block_slider_x) {
    model.applyBC(BC::Dirichlet::FlagOnly(_x), "slider_left");
    model.applyBC(BC::Dirichlet::FlagOnly(_x), "slider_right");
  }
}

void add_restart_files(ap::FixtureWriter & w, const fs::path & dir) {
  std::vector<fs::path> files;
  for (auto & e : fs::directory_iterator(dir)) {
    files.push_back(e.path());
  }
  std::sort(files.begin(), files.end());
  for (auto & f : files) {
    w.add_input_file("restart/" + f.filename().string(), f, "probe output");
  }
}

void case_compression() {
  std::string cid = "compression";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_compression.msh";
  auto mat_file = test_dir() / "data/material_ntn_friction_slip_weakening.dat";
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  Real time_step_factor = getUserParser().getParameter("time_step_factor");
  NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1,
                                  time_step_factor);
  auto contact = solver.getContact();
  auto friction = solver.getFriction();
  apply_fixed_bcs(model, true);
  Vector<Real> trac = {0, -1e6};
  Real simulation_time = 5e-4;
  Real time_step = model.getTimeStep();
  UInt nb_steps = UInt(simulation_time / time_step + 1);
  Real period = 5e-5;
  Vector<Real> trac_nul = {0, 0};
  Recorder r;
  std::vector<long long> steps;
  for (UInt s = 0; s < nb_steps; ++s) {
    if ((s * time_step) < (period / 2)) {
      Vector<Real> trac_t = trac * std::sin(s * time_step / period * 2 * M_PI);
      model.applyBC(BC::Neumann::FromTraction(trac_t), "slider_top");
    } else {
      model.applyBC(BC::Neumann::FromTraction(trac_nul), "slider_top");
    }
    model.solveStep(solver);
    record_contact_state(r, *contact, *friction);
    auto work = solver.getExternalWork();
    r.push("external_work", {work[0], work[1]}, {2}, {"component"});
    r.push("energies",
           {model.getEnergy("kinetic"), model.getEnergy("potential"),
            model.getEnergy("external work new")},
           {3}, {"component"});
    steps.push_back(s);
  }
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "TestNTNFrictionFixture.TestCompression (test_ntn_friction.cc), "
                      "every step, with the restart files",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  w.add_input_file("mesh", mesh_file, upstream_test("ntn_test_compression.msh"));
  w.add_input_file("material", mat_file,
                   upstream_test("data/material_ntn_friction_slip_weakening.dat"));
  auto & p = w.parameters();
  p["time_step"] = time_step;
  p["nb_steps"] = static_cast<long long>(nb_steps);
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{time_step});
  w.set_steps(steps, time_step);
  r.write(w, "step");
  add_array(w, "final/displacement", model.getDisplacement());
  add_array(w, "final/velocity", model.getVelocity());
  add_array(w, "final/acceleration", model.getAcceleration());
  add_array(w, "final/global_contact_pressure", contact->getGlobalContactPressure());
  add_array(w, "final/global_friction_traction", friction->getGlobalFrictionTraction());
  // restart files of the contact and the friction
  auto rdir = g_tmp / "compression_restart";
  fs::remove_all(rdir);
  fs::create_directories(rdir);
  auto prefix = (rdir / "ntn").string();
  contact->dumpRestart(prefix);
  friction->dumpRestart(prefix);
  // UD-394: a fresh friction reading the files back
  auto fresh = initializeNTNFriction(*contact);
  fresh->readRestart(prefix);
  add_array(w, "restart_read/frictional_strength", fresh->getFrictionalStrength().getArray());
  add_array(w, "restart_read/friction_traction", fresh->getFrictionTraction().getArray());
  add_array(w, "restart_read/cumulative_slip", fresh->getCumulativeSlip().getArray());
  add_array(w, "restart_read/slip", fresh->getSlip().getArray());
  // manual displacement restart (manual_restart.cc)
  auto mfile = g_tmp / "compression_displacement.restart";
  dumpRestart(model, mfile.string());
  w.add_input_file("manual_restart", mfile, "probe output: dumpRestart(model, file)");
  add_restart_files(w, rdir);
  w.finish();
}

/* -------------------------------------------------------------------------- */
const char * const kRasFiles[] = {
    "material_ntn_friction_rate_and_state_dieterich_ageing_regularized.dat",
    "material_ntn_friction_rate_and_state_dieterich_ageing.dat",
    "material_ntn_friction_rate_and_state_dieterich_slip.dat",
    "material_ntn_friction_rate_and_state_revised_dieterich_ageing_regularized.dat",
    "material_ntn_friction_rate_and_state_revised_dieterich_ageing.dat",
    "material_ntn_friction_rate_and_state_revised_dieterich_slip.dat",
    "material_ntn_friction_rate_and_state_regularized_dieterich_ageing_regularized.dat",
    "material_ntn_friction_rate_and_state_regularized_dieterich_ageing.dat",
    "material_ntn_friction_rate_and_state_regularized_dieterich_slip.dat",
    "material_ntn_friction_rate_and_state_n_shaped_ageing_regularized.dat",
    "material_ntn_friction_rate_and_state_n_shaped_ageing.dat",
    "material_ntn_friction_rate_and_state_n_shaped_slip.dat",
    "material_ntn_friction_rate_and_state_revised_n_shaped_ageing_regularized.dat",
    "material_ntn_friction_rate_and_state_revised_n_shaped_ageing.dat",
    "material_ntn_friction_rate_and_state_revised_n_shaped_slip.dat",
};

/// Upstream `Velocity` functor of test_ntn_friction_fixture.hh.
class Velocity : public BC::Dirichlet::DirichletFunctor {
public:
  explicit Velocity(SolidMechanicsModel & model, Real vel, BC::Axis ax = _x)
      : DirichletFunctor(ax), model(model), vel(vel) {
    disp = vel * model.getTimeStep();
  }
  inline void operator()(Idx node, VectorProxy<bool> & flags,
                         VectorProxy<Real> & disp,
                         const VectorProxy<const Real> & /*coord*/) override {
    disp(axis) += this->disp;
    model.getVelocity()(node, axis) = vel;
    flags(axis) = true;
  }

private:
  SolidMechanicsModel & model;
  Real vel, disp;
};

void run_rate_and_state(const std::string & cid, const fs::path & mat_file,
                        const std::string & mat_provenance, const std::string & label) {
  auto mesh_file = g_frozen /
                   "extra_packages/traction-at-split-node-contact/test/test_ntn_model/ntn_test_ras.msh";
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  mesh.makePeriodic(_x, "slider_left", "slider_right");
  mesh.makePeriodic(_x, "base_left", "base_right");
  SolidMechanicsModel model(mesh);
  const ParserSection & data = getUserParser();
  Real time_step_factor = data.getParameter("time_step_factor");
  NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1,
                                  time_step_factor);
  auto contact = solver.getContact();
  auto friction = solver.getFriction();
  const auto & mat = model.getMaterial("slider");
  Real shear_vel = data.getParameter("shear_velocity");
  Vector<Real> trac_top = data.getParameter("top_traction");
  Vector<Real> trac_bottom = data.getParameter("bot_traction");
  Real fss = data.getParameter("fss");
  Real E = mat.getParam("E");
  Real nu = mat.getParam("nu");
  Real shear_modulus = E / (2. * (1. + nu));
  Real normal_strain_applied = trac_top(1) / E - nu * nu * trac_top(1) / E;
  Array<Real> & displacement = model.getDisplacement();
  Array<Real> & position = mesh.getNodes();
  UInt nb_nodes = mesh.getNbNodes();
  for (UInt n = 0; n < nb_nodes; ++n) {
    displacement(n, 0) = fss * -trac_top(1) / (shear_modulus)*position(n, 1);
    displacement(n, 1) = normal_strain_applied * position(n, 1);
  }
  model.applyBC(BC::Neumann::FromTraction(trac_top), "slider_top");
  model.applyBC(BC::Neumann::FromTraction(trac_bottom), "base_bottom");
  // fixture setToSteadyState(shear_vel)
  {
    const auto & slider_nodes = mesh.getElementGroup("slider").getNodeGroup().getNodes();
    const auto & base_nodes = mesh.getElementGroup("base").getNodeGroup().getNodes();
    auto & velo = model.getVelocity();
    auto & increment = model.getIncrement();
    auto dt = model.getTimeStep();
    for (auto n : slider_nodes) {
      velo(n, _x) = 0.5 * shear_vel;
      increment(n, _x) = 0.5 * shear_vel * dt;
    }
    for (auto n : base_nodes) {
      velo(n, _x) = -0.5 * shear_vel;
      increment(n, _x) = -0.5 * shear_vel * dt;
    }
    Real phi = friction->get("friction_state");
    auto & is_sticking = friction->getIsSticking();
    for (Int n = 0; n < contact->getNbContactNodes(); ++n) {
      is_sticking(n) = false;
      friction->updateFrictionState(n, phi);
    }
  }
  Real stable_time_step = model.getStableTimeStep();
  Real time_step = stable_time_step * time_step_factor;
  model.setTimeStep(time_step);
  UInt nb_steps = data.getParameter("nb_steps");
  auto apply_velocity = [&]() {
    Velocity velbc_top(model, .5 * shear_vel);
    Velocity velbc_bottom(model, -.5 * shear_vel);
    model.applyBC(velbc_top, "slider_top");
    model.applyBC(velbc_bottom, "base_bottom");
  };
  apply_velocity();
  Recorder every, sampled;
  std::vector<long long> steps;
  for (UInt s = 0; s < nb_steps; ++s) {
    steps.push_back(s);
    model.solveStep(solver, "explicit_lumped");
    auto econ = solver.getExternalWork();
    every.push("energies",
               {model.getEnergy("kinetic"), model.getEnergy("potential"),
                model.getEnergy("external work new"), econ[0], econ[1]},
               {5}, {"component"});
    if (s % 10 == 0) {
      record_contact_state(sampled, *contact, *friction);
    }
    apply_velocity();
  }
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "ParamTestNTNFrictionFixture.TestRateAndState (" + label +
                          "): energies every step, state every 10 steps",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  w.add_input_file("mesh", mesh_file,
                   "frozen gmsh mesh fixtures/meshes/gmsh-4.13.1/extra_packages/"
                   "traction-at-split-node-contact/test/test_ntn_model/ntn_test_ras.msh");
  w.add_input_file("material", mat_file, mat_provenance);
  set_nan_notes(w, cid);
  record_eigen_pin(w);
  auto & p = w.parameters();
  p["time_step"] = time_step;
  p["nb_steps"] = static_cast<long long>(nb_steps);
  p["sample_every"] = 10LL;
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{time_step});
  w.set_steps(steps, time_step);
  every.write(w, "step");
  for (auto & [name, s] : sampled.series) {
    auto ser = s;
    std::vector<std::size_t> shape{ser.rows};
    shape.insert(shape.end(), ser.inner.begin(), ser.inner.end());
    std::vector<std::string> axes{"sample"};
    axes.insert(axes.end(), ser.axes.begin(), ser.axes.end());
    w.add("sampled/" + name, shape, axes, "table", ser.values);
  }
  add_array(w, "final/displacement", model.getDisplacement());
  add_array(w, "final/velocity", model.getVelocity());
  w.finish();
}

void case_rate_and_state(const std::string & file) {
  std::string stem = file.substr(0, file.size() - 4); // drop .dat
  std::string cid = "ras_" + stem.substr(std::string("material_ntn_friction_rate_and_state_").size());
  if (not selected(cid)) {
    return;
  }
  run_rate_and_state(cid, test_dir() / "data" / file, upstream_test("data/" + file), file);
}

// The n_shaped slip state law (and its legacy key), never run by
// upstream's data (its n_shaped_slip file runs n_shaped:ageing, UD-405):
// upstream's n_shaped_slip data with only the friction line replaced.
void case_rate_and_state_law(const std::string & cid, const std::string & law) {
  if (not selected(cid)) {
    return;
  }
  const std::string file = "material_ntn_friction_rate_and_state_n_shaped_slip.dat";
  const std::string from = "friction rate_and_state:n_shaped:ageing ";
  std::ifstream in(test_dir() / "data" / file);
  std::stringstream ss;
  ss << in.rdbuf();
  std::string text = ss.str();
  auto pos = text.find(from);
  if (pos == std::string::npos or text.find(from, pos + 1) != std::string::npos) {
    throw std::runtime_error("unexpected friction line in " + file);
  }
  text.replace(pos, from.size(), "friction " + law + " ");
  auto mat_file = write_text(cid + ".dat", text);
  run_rate_and_state(cid, mat_file,
                     "probe-generated: " + upstream_test("data/" + file) +
                         " with the friction line 'rate_and_state:n_shaped:ageing' replaced by '" +
                         law + "'",
                     file + " with " + law);
}

/* -------------------------------------------------------------------------- */
struct LawCase {
  std::string id;
  std::string law;
  std::string reg;
  std::string params;
};

std::vector<LawCase> law_cases() {
  const std::string rs = "mu_0 = 0.28\n a = 0.005\n b = 0.075\n v_star = 1e-7\n "
                         "phi_star = 0.00033\n d_c = 5e-7\n v0 = 1e-7\n "
                         "friction_state = 0.00025\n";
  struct L {
    std::string short_name, law, params;
  };
  std::vector<L> laws = {
      {"coulomb", "coulomb", "mu = 0.2\n"},
      {"lsw", "linear_slip_weakening", "mu_s = 0.25\n mu_k = 0.1\n d_c = 1e-6\n"},
      {"lswnh", "linear_slip_weakening_no_healing",
       "mu_s = 0.25\n mu_k = 0.1\n d_c = 1e-6\n"},
      {"cohesive", "linear_cohesive", "G_c = 0.1\n tau_c = 3e5\n tau_r = 1e5\n"},
      {"ras_regdieterich_ageing", "rate_and_state:regularized_dieterich:ageing", rs},
      {"ras_flashheating", "rate_and_state:flashheating", rs + " mu_w = 0.1\n v_w = 0.1\n"},
  };
  struct R {
    std::string short_name, reg, params;
  };
  std::vector<R> regs = {
      {"none", "no_regularisation", ""},
      {"ra", "rubin_ampuero", "t_star = 2e-5\n"},
      {"spc", "simplified_prakash_clifton", "t_star = 2e-5\n"},
  };
  std::vector<LawCase> out;
  for (auto & l : laws) {
    for (auto & r : regs) {
      out.push_back({"law_" + l.short_name + "__" + r.short_name, l.law, r.reg,
                     l.params + " " + r.params});
    }
  }
  // UD-039: Dieterich at rest (sticking nodes, zero slip velocity)
  out.push_back({"law_dieterich_ageing_at_rest__none", "rate_and_state:dieterich:ageing",
                 "no_regularisation", rs});
  // UD-397: simplified Prakash-Clifton with its default t_star = 0
  out.push_back({"law_coulomb__spc_tstar0", "coulomb", "simplified_prakash_clifton",
                 "mu = 0.2\n"});
  return out;
}

void case_law(const LawCase & c) {
  if (not selected(c.id)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_compression.msh";
  std::string dat = "friction " + c.law + " " + c.reg + " [\n " + c.params + "\n]\n" +
                    kMaterials;
  auto mat_file = write_text(c.id + ".dat", dat);
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1, 0.1);
  auto contact = solver.getContact();
  auto friction = solver.getFriction();
  apply_fixed_bcs(model, false);
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{0., -1e6}), "slider_top");
  Recorder r;
  std::vector<long long> steps;
  const UInt nb_steps = 150;
  for (UInt s = 0; s < nb_steps; ++s) {
    if (s >= 40) {
      model.applyBC(BC::Neumann::FromTraction(Vector<Real>{4e3, 0.}), "slider_top");
    }
    model.solveStep(solver);
    record_contact_state(r, *contact, *friction);
    auto work = solver.getExternalWork();
    r.push("external_work", {work[0], work[1]}, {2}, {"component"});
    if (s % 10 == 0) {
      r.push_array("displacement_every_10", model.getDisplacement());
    }
    steps.push_back(s);
  }
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + c.id,
                      "probe scenario: compression then tangential traction ramp, " +
                          c.law + " / " + c.reg,
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  set_nan_notes(w, c.id);
  w.add_input_file("mesh", mesh_file, upstream_test("ntn_test_compression.msh"));
  w.add_input_file("material", mat_file, "probe-generated");
  auto & p = w.parameters();
  p["law"] = c.law;
  p["regularisation"] = c.reg;
  p["time_step"] = model.getTimeStep();
  p["nb_steps"] = static_cast<long long>(nb_steps);
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{model.getTimeStep()});
  p["shear_start"] = 40LL;
  p["shear_increment"] = 4e3;
  p["normal_traction"] = -1e6;
  std::ostringstream os;
  friction->printself(os);
  p["friction_printself"] = class_chain(os.str());
  w.set_steps(steps, model.getTimeStep());
  // displacement_every_10 has its own rows
  auto every10 = r.series["displacement_every_10"];
  r.series.erase("displacement_every_10");
  r.write(w, "step");
  {
    std::vector<std::size_t> shape{every10.rows};
    shape.insert(shape.end(), every10.inner.begin(), every10.inner.end());
    w.add("displacement_every_10", shape, {"sample", "entry", "component"}, "table",
          every10.values);
  }
  w.finish();
}

/* -------------------------------------------------------------------------- */
void case_ntrf() {
  std::string cid = "ntrf_coulomb";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_compression.msh";
  auto mat_file = write_text(cid + ".dat",
                             "friction coulomb rubin_ampuero [\n mu = 0.2\n t_star = 2e-5\n]\n" +
                                 kMaterials);
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  NTRFContact contact(model);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  model.assembleMassLumped();
  contact.setReferencePoint(0., 0., 0.);
  contact.setNormal(0., 1., 0.);
  contact.addSurface("slider_bottom");
  Real dt = model.getStableTimeStep() * 0.1;
  model.setTimeStep(dt);
  contact.updateImpedance();
  auto friction = initializeNTNFriction(contact);
  ProbeCallback cb(model, contact, *friction);
  apply_fixed_bcs(model, false);
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{0., -1e6}), "slider_top");
  Recorder r;
  std::vector<long long> steps;
  const UInt nb_steps = 150;
  for (UInt s = 0; s < nb_steps; ++s) {
    if (s >= 40) {
      model.applyBC(BC::Neumann::FromTraction(Vector<Real>{4e3, 0.}), "slider_top");
    }
    model.solveStep(cb);
    record_contact_state(r, contact, *friction);
    r.push("external_work", {cb.external_work[0], cb.external_work[1]}, {2}, {"component"});
    steps.push_back(s);
  }
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "probe scenario: NTRFContact (rigid plane y = 0, normal +y) on "
                      "slider_bottom with NTRFFriction<coulomb, rubin_ampuero>",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  w.add_input_file("mesh", mesh_file, upstream_test("ntn_test_compression.msh"));
  w.add_input_file("material", mat_file, "probe-generated");
  auto & p = w.parameters();
  p["time_step"] = dt;
  p["nb_steps"] = static_cast<long long>(nb_steps);
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{dt});
  std::ostringstream os;
  friction->printself(os);
  p["friction_printself"] = class_chain(os.str());
  w.add("slaves", {static_cast<std::size_t>(contact.getNbContactNodes())},
        {"entry"}, "table", ivec_of(contact.getSlaves().getArray()));
  add_array(w, "lumped_boundary_slaves", contact.getLumpedBoundarySlaves().getArray());
  add_array(w, "impedance", contact.getImpedance().getArray());
  w.set_steps(steps, dt);
  r.write(w, "step");
  add_array(w, "final/displacement", model.getDisplacement());
  add_array(w, "final/global_contact_pressure", contact.getGlobalContactPressure());
  add_array(w, "final/global_friction_traction", friction->getGlobalFrictionTraction());
  w.finish();
}

/* -------------------------------------------------------------------------- */
void case_miiasym() {
  std::string cid = "miiasym_fields";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_compression.msh";
  auto mat_file = write_text(cid + ".dat", kMaterials);
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  MIIASYMContact contact(model);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  model.assembleMassLumped();
  contact.setReferencePoint(0., 0.05, 0.);
  contact.setNormal(0.6, 0.8, 0.);
  contact.addSurface("slider_bottom");
  Real dt = model.getStableTimeStep() * 0.1;
  model.setTimeStep(dt);
  contact.updateImpedance();
  // prescribed fields
  auto & disp = model.getDisplacement();
  auto & velo = model.getVelocity();
  auto & acce = model.getAcceleration();
  const auto & X = mesh.getNodes();
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    disp(n, 0) = 1e-4 * X(n, 1) + 2e-5 * X(n, 0);
    disp(n, 1) = -3e-4 * X(n, 1) + 1e-5;
    velo(n, 0) = 0.1 * X(n, 1) + 0.01 * X(n, 0);
    velo(n, 1) = -0.2 * X(n, 0) - 0.05;
    acce(n, 0) = 3. * X(n, 0) * X(n, 1);
    acce(n, 1) = -7. + X(n, 0);
  }
  model.assembleInternalForces();
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "probe scenario: MIIASYMContact fields and equilibrium pressure",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  w.add_input_file("mesh", mesh_file, upstream_test("ntn_test_compression.msh"));
  w.add_input_file("material", mat_file, "probe-generated");
  auto & p = w.parameters();
  p["time_step"] = dt;
  add_array(w, "impedance", contact.getImpedance().getArray());
  add_array(w, "normals", contact.getNormals().getArray());
  Array<Real> rn(0, 1), rt(0, 2), gap(0, 1);
  contact.computeRelativeNormalField(velo, rn);
  contact.computeRelativeTangentialField(velo, rt);
  contact.computeNormalGap(gap);
  add_array(w, "relative_normal_velocity", rn);
  add_array(w, "relative_tangential_velocity", rt);
  add_array(w, "normal_gap", gap);
  contact.computeContactPressure(); // no-op
  add_array(w, "pressure_after_noop", contact.getContactPressure().getArray());
  contact.computeContactPressureInEquilibrium();
  add_array(w, "pressure_in_equilibrium", contact.getContactPressure().getArray());
  add_array(w, "is_in_contact", contact.getIsInContact().getArray());
  contact.assembleGlobalContactPressure();
  add_array(w, "global_contact_pressure", contact.getGlobalContactPressure());
  set_nan_notes(w, cid);
  w.finish();
}

/* -------------------------------------------------------------------------- */
std::vector<std::string> registered_laws() {
  std::vector<std::string> v;
  for (auto c : {"dieterich", "revised_dieterich", "regularized_dieterich", "n_shaped",
                 "revised_n_shaped"}) {
    for (auto s : {"ageing", "slip", "ageing_regularized"}) {
      v.push_back(std::string("rate_and_state:") + c + ":" + s);
    }
  }
  for (auto s : {"ageing", "slip", "ageing_regularized"}) {
    v.push_back(std::string("rate_and_state_n_shaped_") + s);
  }
  v.push_back("rate_and_state:flashheating");
  for (auto l : {"coulomb", "linear_slip_weakening", "linear_slip_weakening_no_healing",
                 "linear_cohesive"}) {
    v.push_back(l);
  }
  return v;
}

void case_factory() {
  std::string cid = "factory_selection";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_2d.msh";
  auto mat_file = test_dir() / "data/material_ntn_model.dat";
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "printself of initializeNTNFriction for every law x regularisation "
                      "option, node-to-node and rigid-flat contacts",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  w.add_input_file("mesh", mesh_file, upstream_test("ntn_test_2d.msh"));
  auto & p = w.parameters();
  ap::Json cases = ap::Json::array();
  for (bool ntn : {true, false}) {
    for (auto & law : registered_laws()) {
      for (auto reg : {"no_regularisation", "rubin_ampuero", "simplified_prakash_clifton",
                       "bogus_option"}) {
        auto dat = write_text("factory.dat", "friction " + law + " " + reg + " [\n]\n" +
                                                 "material elastic [\n name = a\n rho = 1\n"
                                                 " E = 1\n nu = 0\n]\n");
        Mesh mesh(2);
        mesh.read(mesh_file.string());
        getStaticParser().parse(dat.string());
        SolidMechanicsModel model(mesh);
        model.initFull(_analysis_method = _explicit_lumped_mass);
        model.assembleMassLumped();
        std::unique_ptr<NTNBaseContact> contact;
        if (ntn) {
          auto c = std::make_unique<NTNContact>(model);
          c->addSurfacePair("slider_bottom", "base_top", 1);
          contact = std::move(c);
        } else {
          auto c = std::make_unique<NTRFContact>(model);
          c->setNormal(0., 1., 0.);
          c->addSurface("slider_bottom");
          contact = std::move(c);
        }
        auto friction = initializeNTNFriction(*contact);
        std::ostringstream os;
        friction->printself(os);
        ap::Json one = ap::Json::object();
        one["contact"] = std::string(ntn ? "ntn" : "ntrf");
        one["law"] = law;
        one["option"] = std::string(reg);
        one["printself"] = class_chain(os.str());
        cases.push(one);
      }
    }
  }
  p["cases"] = cases;
  (void)mat_file;
  w.finish();
}

/* -------------------------------------------------------------------------- */
void case_parameter_reader() {
  std::string cid = "parameter_reader";
  if (not selected(cid)) {
    return;
  }
  std::string text =
      "# comment line\n"
      "REAL dt = 1.5e-3 # trailing comment\n"
      "real x=2\n"
      "real y =3.5\n"
      "real z= 4.25\n"
      "real third = 0.333333333333333314829616256247\n"
      "real big = 6.02214076e23\n"
      "int n = 3.7\n"
      "int bad = abc\n"
      "uint u = -1\n"
      "int neg = -42\n"
      "bool flag = TRUE\n"
      "bool off = false\n"
      "bool other = maybe\n"
      "string name = slider_top extra\n"
      "elementtype et = _quadrangle_4\n"
      "elementtype nope = _pentahedron_6\n"
      "foo bar = 1\n"
      "real noequal 3\n"
      "real dt = 9\n"
      "real empty =\n";
  auto in = write_text("parameter_reader_input.txt", text);
  ParameterReader pr;
  pr.readInputFile(in.string());
  auto out = g_tmp / "parameter_reader_output.txt";
  pr.writeInputFile(out.string());
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "ParameterReader readInputFile / writeInputFile / get<T>", kProbe,
                      kProbeVersion);
  w.add_tag("ntn_contact");
  w.add_input_file("input", in, "probe-generated");
  w.add_input_file("written", out, "probe output: writeInputFile");
  auto & p = w.parameters();
  p["dt"] = pr.get<Real>("dt");
  p["x"] = pr.get<Real>("x");
  p["third"] = pr.get<Real>("third");
  p["big"] = pr.get<Real>("big");
  p["n"] = static_cast<long long>(pr.get<Int>("n"));
  p["bad"] = static_cast<long long>(pr.get<Int>("bad"));
  p["u"] = static_cast<long long>(pr.get<UInt>("u"));
  p["neg"] = static_cast<long long>(pr.get<Int>("neg"));
  p["flag"] = pr.get<bool>("flag");
  p["name"] = pr.get<std::string>("name");
  p["has_other"] = pr.has<bool>("other");
  p["has_noequal"] = pr.has<Real>("noequal");
  p["has_empty"] = pr.has<Real>("empty");
  w.finish();
}

/* -------------------------------------------------------------------------- */
void case_bc_spring() {
  std::string cid = "bc_spring";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_compression.msh";
  auto mat_file = write_text(cid + ".dat", kMaterials);
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  Real dt = model.getStableTimeStep() * 0.5;
  model.setTimeStep(dt);
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_bottom");
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{0., -1e5}), "slider_left");
  SpringBC spring(model, _y, 1e9, 10.);
  spring.insertSubBoundary("slider_top");
  Recorder r;
  std::vector<long long> steps;
  for (UInt s = 0; s < 60; ++s) {
    model.solveStep();
    Real reaction = spring.applyYourself();
    r.push("spring", {reaction, spring.getElongation()}, {2}, {"component"});
    steps.push_back(s);
  }
  // force-based Dirichlet with a target force, same model
  ForceBasedDirichlet fbd(model, _x, 5e3, 20.);
  fbd.insertSubBoundary("slider_right");
  for (UInt s = 0; s < 20; ++s) {
    model.solveStep();
    Real reaction = fbd.applyYourself();
    r.push("force_based", {reaction}, {1}, {"component"});
  }
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "probe scenario: SpringBC on slider_top then ForceBasedDirichlet "
                      "on slider_right, explicit lumped",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("ntn_contact");
  w.add_input_file("mesh", mesh_file, upstream_test("ntn_test_compression.msh"));
  w.add_input_file("material", mat_file, "probe-generated");
  auto & p = w.parameters();
  p["time_step"] = dt;
  // series of different lengths
  auto spring_series = r.series["spring"];
  auto fbd_series = r.series["force_based"];
  w.add("spring", {spring_series.rows, 2}, {"step", "component"}, "table",
        spring_series.values);
  w.add("force_based", {fbd_series.rows, 1}, {"step", "component"}, "table",
        fbd_series.values);
  add_array(w, "final/displacement", model.getDisplacement());
  w.finish();
}

// InclinedFlatDirichlet (UD-403) is not probed: `model.applyBC(bc, group)`
// does not compile at this commit (boundary_condition_tmpl.hh calls the
// functor with VectorProxy arguments, which cannot bind to its
// `Vector<...> &` parameters), so there is no upstream behaviour to record.

} // namespace

int main(int argc, char ** argv) {
  if (argc != 5) {
    std::cerr << "usage: ntn_probe <fixtures_root> <upstream_src_root> "
                 "<frozen_meshes_root> <tmp_dir>\n";
    return 2;
  }
  // D-050: host-independent blocking of Eigen's dense kernels
  Eigen::setCpuCacheSizes(kEigenL1, kEigenL2, kEigenL3);
  initialize(argc, argv);
  g_root = argv[1];
  g_src = argv[2];
  g_frozen = argv[3];
  g_tmp = argv[4];
  fs::create_directories(g_tmp);
  if (const char * only = std::getenv("NTN_PROBE_CASES")) {
    std::stringstream ss(only);
    std::string id;
    while (std::getline(ss, id, ',')) {
      g_only.insert(id);
    }
  }
  case_basics(2);
  case_basics(3);
  case_compression();
  for (auto * f : kRasFiles) {
    case_rate_and_state(f);
  }
  case_rate_and_state_law("ras_n_shaped_slip_law", "rate_and_state:n_shaped:slip");
  case_rate_and_state_law("ras_legacy_n_shaped_slip", "rate_and_state_n_shaped_slip");
  for (auto & c : law_cases()) {
    case_law(c);
  }
  case_ntrf();
  case_miiasym();
  case_factory();
  case_parameter_reader();
  case_bc_spring();
  finalize();
  return 0;
}
