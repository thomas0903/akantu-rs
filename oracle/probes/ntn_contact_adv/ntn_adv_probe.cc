// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Adversarial oracle
// probe (not a port), written
// (artifacts adv_probe/adv_probe.cc)
// and promoted to a golden generator: drives the unmodified upstream
// traction-at-split-node-contact classes in scenarios that the probe
// (oracle/probes/ntn_contact) does not cover and records them as
// fixtures/model/ntn_contact/adv_* for the replay test
// crates/akantu-ntn-contact/tests/oracle_ntn_adversarial.rs.
//
//   adv_stale_position   2-D, NTNContactSolverCallback (coulomb):
//       displacement written before the first step, then updateNormals and
//       computeNormalGap (the model's current position is still X: release
//       0 == 0); 30 steps; displacement written twice between steps, with
//       updateNormals/computeNormalGap after each write (the first write is
//       seen because the corrector changed the release, the second is not);
//       30 more steps.
//   adv_3d_ras_reversal_dc, adv_3d_ras_reversal
//       3-D (ntn_test_3d.msh), rate_and_state:
//       regularized_dieterich:ageing with rubin_ampuero: normal load (tension:
//       both interface faces of the test mesh are oriented inward, UD-707),
//       then a tangential traction (+x, +z) from step 40, reversed (-x, -z)
//       from step 100 (each load applied once: applyBC(Neumann) accumulates);
//       200 steps, every step recorded. _dc: d_c = 1e-3 (stable: stick, slip,
//       stick, reversed slip); without suffix: d_c = 5e-7, the explicit-Euler
//       ageing update is unstable (V dt/d_c > 1) and upstream strengths turn
//       NaN from step 96 (UD-039).
//   adv_restart_continue 2-D, linear_slip_weakening + rubin_ampuero: run A
//       60 steps, dump the contact/friction restart files, the displacement
//       (manual restart) and velocity/acceleration (dumpArray), continue A
//       40 steps; run B: fresh model/callback, load displacement, velocity,
//       acceleration, readRestart of the contact and the friction (UD-394:
//       no value restored upstream), 40 steps.
//
// Cases whose upstream run produces NaN carry a manifest note (D-046 (4),
// table kNanNotes). Eigen's cache sizes are pinned to its defaults at the
// start of main (D-050; these explicit lumped-mass runs solve no Eigen system)
// and recorded as parameters.eigen_cache_sizes.
//
// Usage: ntn_adv_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root> <tmp_dir>
// Environment: NTN_PROBE_CASES=<id>[,<id>...] restricts the run to the listed
// case ids (without the model/ntn_contact/ prefix).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <mesh.hh>
#include <solid_mechanics_model.hh>

#include <manual_restart.hh>
#include <ntn_base_contact.hh>
#include <ntn_base_friction.hh>
#include <ntn_contact.hh>
#include <ntn_contact_solvercallback.hh>
#include <ntn_initiation_function.hh>

#include <Eigen/Core>

#include <algorithm>
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
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

const char * const kProbe = "ntn_adv_probe";
const char * const kProbeVersion = "1";

fs::path g_root, g_src, g_frozen, g_tmp;
std::set<std::string> g_only;

bool selected(const std::string & id) { return g_only.empty() or g_only.count(id) > 0; }

// D-050: Eigen's own default cache sizes (host-independent blocking).
constexpr std::ptrdiff_t kEigenL1 = 16384, kEigenL2 = 524288, kEigenL3 = 524288;

// D-046 (4): every case whose upstream run produces NaN documents it (which
// quantities, from which step, why) in its manifest notes.
const std::map<std::string, std::string> kNanNotes = {
      {"adv_3d_ras_reversal",
       "NaN (D-046): frictional_strength holds default quiet NaNs of invalid operations in 633 of 1800 values, from step 96. Cause (UD-039, TN-11): upstream's explicit-Euler ageing update theta <- theta (1 - V dt/d_c) + dt is unstable when V dt/d_c > 1 (d_c = 5e-7 here); the state of sliding nodes turns negative below -phi_star, ln(1 + theta/phi_star) of the regularized Dieterich coefficient is NaN, and those nodes then count as sticking. Reproduced in upstream-compat mode; the port's corrected default stops at step 96 with the UD-039 domain error (negative state)."},
      {"adv_restart_continue",
       "NaN (D-046): b_after_read/contact_pressure holds 22 values 0x7ff8000000000000, std::numeric_limits<Real>::quiet_NaN() (a constant: the same bits on every platform): the fresh contact of run B still has the contact pressure that NTNBaseContact::addSplitNode initialises with quiet_NaN, because readRestart restores no value upstream (UD-394) and no step has run yet. The port's corrected default restores the values."},
};

// Tags, D-046 notes and the D-050 pin of every case.
void common_metadata(ap::FixtureWriter & w, const std::string & cid) {
  w.add_tag("model");
  w.add_tag("ntn_contact");
  auto it = kNanNotes.find(cid);
  if (it != kNanNotes.end()) {
    w.set_notes(it->second);
  }
  ap::Json c = ap::Json::array();
  c.push(static_cast<long long>(kEigenL1));
  c.push(static_cast<long long>(kEigenL2));
  c.push(static_cast<long long>(kEigenL3));
  w.parameters()["eigen_cache_sizes"] = c;
}

fs::path test_dir() {
  return g_src / "extra_packages/traction-at-split-node-contact/test/test_ntn_model";
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

struct Recorder {
  struct Series {
    std::vector<double> values;
    std::vector<std::size_t> inner;
    std::vector<std::string> axes;
    std::size_t rows{0};
  };
  std::map<std::string, Series> series;
  void push(const std::string & name, const std::vector<double> & v,
            const std::vector<std::size_t> & inner, const std::vector<std::string> & axes) {
    auto & s = series[name];
    if (s.rows == 0) {
      s.inner = inner;
      s.axes = axes;
    }
    s.values.insert(s.values.end(), v.begin(), v.end());
    s.rows += 1;
  }
  template <class T> void push_array(const std::string & name, const Array<T> & a) {
    push(name, vec_of(a),
         {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
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

template <class T>
void add_array(ap::FixtureWriter & w, const std::string & name, const Array<T> & a) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a));
}

fs::path write_text(const std::string & name, const std::string & text) {
  auto p = g_tmp / name;
  std::ofstream f(p);
  f << text;
  return p;
}

void record_contact_state(Recorder & r, const std::string & prefix, NTNBaseContact & contact,
                          NTNBaseFriction & friction) {
  r.push_array(prefix + "contact_pressure", contact.getContactPressure().getArray());
  r.push_array(prefix + "is_in_contact", contact.getIsInContact().getArray());
  r.push_array(prefix + "friction_traction", friction.getFrictionTraction().getArray());
  r.push_array(prefix + "frictional_strength", friction.getFrictionalStrength().getArray());
  r.push_array(prefix + "is_sticking", friction.getIsSticking().getArray());
  r.push_array(prefix + "slip", friction.getSlip().getArray());
  r.push_array(prefix + "cumulative_slip", friction.getCumulativeSlip().getArray());
  r.push_array(prefix + "slip_velocity_norm", friction.getSlipVelocityNorm().getArray());
  r.push_array(prefix + "friction_state", friction.getState().getArray());
}

const std::string kMaterials2d = R"(
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

const std::string kMaterials3d = R"(
material elastic [
     name = slider
     rho  = 1200
     nu   = 0.33
     E    = 7.98e9
]

material elastic [
     name = base
     rho  = 1200
     nu   = 0.33
     E    = 7.98e9
]
)";

void fixed_bcs_2d(SolidMechanicsModel & model) {
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_right");
}

/* -------------------------------------------------------------------------- */
void case_stale_position() {
  std::string cid = "adv_stale_position";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_compression.msh";
  auto mat_file =
      write_text(cid + ".dat", "friction coulomb no_regularisation [\n mu = 0.2\n]\n" + kMaterials2d);
  Mesh mesh(2);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1, 0.1);
  auto contact = solver.getContact();
  auto friction = solver.getFriction();
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "review V72: displacement written between steps vs the release-cached "
                      "current position (updateNormals, computeNormalGap)",
                      kProbe, kProbeVersion);
  const auto & X = mesh.getNodes();
  auto & disp = model.getDisplacement();
  // write 0: before any step (release 0 == current_position_release 0)
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    disp(n, 0) = 1e-4 * X(n, 1);
    disp(n, 1) = 2e-3 * X(n, 0) * X(n, 0) - 1e-5;
  }
  contact->updateNormals();
  add_array(w, "w0/normals", contact->getNormals().getArray());
  {
    Array<Real> gap(0, 1);
    contact->computeNormalGap(gap);
    add_array(w, "w0/gap", gap);
  }
  add_array(w, "w0/current_position", model.getCurrentPosition());
  fixed_bcs_2d(model);
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{0., -1e6}), "slider_top");
  Recorder r;
  for (UInt s = 0; s < 30; ++s) {
    model.solveStep(solver);
    record_contact_state(r, "", *contact, *friction);
  }
  // write 1 (seen: the corrector changed the release after the last refresh)
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    disp(n, 1) += 1e-4 * X(n, 0);
  }
  contact->updateNormals();
  add_array(w, "w1/normals", contact->getNormals().getArray());
  {
    Array<Real> gap(0, 1);
    contact->computeNormalGap(gap);
    add_array(w, "w1/gap", gap);
  }
  // write 2 (not seen: no release change since the refresh of write 1)
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    disp(n, 1) += 3e-4 * X(n, 0) * X(n, 0);
  }
  contact->updateNormals();
  add_array(w, "w2/normals", contact->getNormals().getArray());
  {
    Array<Real> gap(0, 1);
    contact->computeNormalGap(gap);
    add_array(w, "w2/gap", gap);
  }
  add_array(w, "w2/current_position", model.getCurrentPosition());
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{4e3, -1e6}), "slider_top");
  for (UInt s = 30; s < 60; ++s) {
    model.solveStep(solver);
    record_contact_state(r, "", *contact, *friction);
  }
  common_metadata(w, cid);
  w.add_input_file("mesh", mesh_file, "upstream test mesh ntn_test_compression.msh");
  w.add_input_file("material", mat_file, "probe-generated");
  w.parameters()["time_step"] = model.getTimeStep();
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{model.getTimeStep()});
  r.write(w, "step");
  add_array(w, "final/displacement", model.getDisplacement());
  add_array(w, "final/velocity", model.getVelocity());
  add_array(w, "final/normals", contact->getNormals().getArray());
  w.finish();
}

/* -------------------------------------------------------------------------- */
void case_3d_ras_reversal(const std::string & cid, const std::string & d_c,
                          const std::string & state0) {
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_3d.msh";
  auto mat_file = write_text(
      cid + ".dat",
      "friction rate_and_state:regularized_dieterich:ageing rubin_ampuero [\n"
      " mu_0 = 0.28\n a = 0.005\n b = 0.075\n v_star = 1e-7\n phi_star = 0.00033\n"
      " d_c = " + d_c + "\n v0 = 1e-7\n friction_state = " + state0 + "\n t_star = 2e-5\n]\n" +
          kMaterials3d);
  Mesh mesh(3);
  mesh.read(mesh_file.string());
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1, 0.1);
  auto contact = solver.getContact();
  auto friction = solver.getFriction();
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_z), "base_bottom");
  Recorder r;
  // applyBC(Neumann) adds to the external force: each load is applied once.
  // The faces of ntn_test_3d.msh are oriented inward (base_top normal -y), so
  // the contact normal points into the base and the "contact" closes under
  // normal *tension*: pull the slider (+y) to exercise the 3-D friction.
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{0., 1e6, 0.}), "slider_top");
  for (UInt s = 0; s < 200; ++s) {
    if (s == 40) {
      model.applyBC(BC::Neumann::FromTraction(Vector<Real>{4e5, 0., 2e5}), "slider_top");
    } else if (s == 100) {
      model.applyBC(BC::Neumann::FromTraction(Vector<Real>{-9e5, 0., -4.5e5}), "slider_top");
    }
    model.solveStep(solver);
    record_contact_state(r, "", *contact, *friction);
    auto work = solver.getExternalWork();
    r.push("external_work", {work[0], work[1]}, {2}, {"component"});
    r.push("energies",
           {model.getEnergy("kinetic"), model.getEnergy("potential"),
            model.getEnergy("external work new")},
           {3}, {"component"});
  }
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "review V72: 3-D node-to-node, R&S regularized Dieterich ageing + "
                      "Rubin-Ampuero, slip reversal",
                      kProbe, kProbeVersion);
  common_metadata(w, cid);
  w.add_input_file("mesh", mesh_file, "upstream test mesh ntn_test_3d.msh");
  w.add_input_file("material", mat_file, "probe-generated");
  w.parameters()["time_step"] = model.getTimeStep();
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{model.getTimeStep()});
  add_array(w, "normals", contact->getNormals().getArray());
  add_array(w, "impedance", contact->getImpedance().getArray());
  r.write(w, "step");
  add_array(w, "final/displacement", model.getDisplacement());
  add_array(w, "final/velocity", model.getVelocity());
  add_array(w, "final/global_friction_traction", friction->getGlobalFrictionTraction());
  w.finish();
}

/* -------------------------------------------------------------------------- */
void restart_bcs(SolidMechanicsModel & model) {
  fixed_bcs_2d(model);
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{0., -1e6}), "slider_top");
}

void case_restart_continue() {
  std::string cid = "adv_restart_continue";
  if (not selected(cid)) {
    return;
  }
  auto mesh_file = test_dir() / "ntn_test_compression.msh";
  auto mat_file = write_text(cid + ".dat",
                             "friction linear_slip_weakening rubin_ampuero [\n mu_s = 0.25\n"
                             " mu_k = 0.1\n d_c = 1e-6\n t_star = 2e-5\n]\n" +
                                 kMaterials2d);
  auto rdir = g_tmp / "adv_restart";
  fs::remove_all(rdir);
  fs::create_directories(rdir);
  auto prefix = (rdir / "ntn").string();
  auto dfile = rdir / "displacement.restart";
  auto vfile = rdir / "velocity.restart";
  auto afile = rdir / "acceleration.restart";
  Recorder r;
  // run A
  {
    Mesh mesh(2);
    mesh.read(mesh_file.string());
    getStaticParser().parse(mat_file.string());
    SolidMechanicsModel model(mesh);
    NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1, 0.1);
    auto contact = solver.getContact();
    auto friction = solver.getFriction();
    restart_bcs(model);
    for (UInt s = 0; s < 100; ++s) {
      if (s == 20) {
        model.applyBC(BC::Neumann::FromTraction(Vector<Real>{3e5, 0.}), "slider_top");
      }
      model.solveStep(solver);
      if (s == 59) {
        contact->dumpRestart(prefix);
        friction->dumpRestart(prefix);
        dumpRestart(model, dfile.string());
        dumpArray(model.getVelocity(), vfile.string());
        dumpArray(model.getAcceleration(), afile.string());
        record_contact_state(r, "a_at_restart/", *contact, *friction);
      }
      if (s >= 60) {
        record_contact_state(r, "a/", *contact, *friction);
        auto work = solver.getExternalWork();
        r.push("a/external_work", {work[0], work[1]}, {2}, {"component"});
      }
    }
    r.push_array("a/final_displacement", model.getDisplacement());
  }
  // run B
  {
    Mesh mesh(2);
    mesh.read(mesh_file.string());
    getStaticParser().parse(mat_file.string());
    SolidMechanicsModel model(mesh);
    NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1, 0.1);
    auto contact = solver.getContact();
    auto friction = solver.getFriction();
    restart_bcs(model);
    loadRestart(model, dfile.string());
    loadArray(model.getVelocity(), vfile.string());
    loadArray(model.getAcceleration(), afile.string());
    contact->readRestart(prefix);
    friction->readRestart(prefix);
    // same total external force as run A at step 60
    model.applyBC(BC::Neumann::FromTraction(Vector<Real>{3e5, 0.}), "slider_top");
    record_contact_state(r, "b_after_read/", *contact, *friction);
    for (UInt s = 60; s < 100; ++s) {
      model.solveStep(solver);
      record_contact_state(r, "b/", *contact, *friction);
      auto work = solver.getExternalWork();
      r.push("b/external_work", {work[0], work[1]}, {2}, {"component"});
    }
    r.push_array("b/final_displacement", model.getDisplacement());
  }
  ap::FixtureWriter w(g_root, "model/ntn_contact/" + cid,
                      "review V72: restart mid-run (contact/friction restart files + manual "
                      "displacement restart) and continue vs uninterrupted",
                      kProbe, kProbeVersion);
  common_metadata(w, cid);
  w.add_input_file("mesh", mesh_file, "upstream test mesh ntn_test_compression.msh");
  w.add_input_file("material", mat_file, "probe-generated");
  std::vector<fs::path> files;
  for (auto & e : fs::directory_iterator(rdir)) {
    files.push_back(e.path());
  }
  std::sort(files.begin(), files.end());
  for (auto & f : files) {
    w.add_input_file("restart/" + f.filename().string(), f, "probe output");
  }
  r.write(w, "row");
  w.finish();
}

} // namespace

int main(int argc, char ** argv) {
  if (argc != 5) {
    std::cerr << "usage: ntn_adv_probe <fixtures_root> <upstream_src_root> "
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
  case_stale_position();
  // d_c = 5e-7: explicit Euler ageing state unstable (V dt / d_c > 1), NaN
  // strengths upstream; d_c = 1e-3: stable
  case_3d_ras_reversal("adv_3d_ras_reversal", "5e-7", "0.00025");
  case_3d_ras_reversal("adv_3d_ras_reversal_dc", "1e-3", "0.5");
  case_restart_continue();
  finalize();
  return 0;
}
