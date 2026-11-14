// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/node_to_node/steady_state.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:`. Changes: Eigen's cache sizes are pinned at the start of
// main (D-050); `energies_full.csv`: after every step the energies line of
// `friction-energies-steady_state.csv` with 17 significant digits, the time
// step and the SHA-256 of the raw little-endian bytes of displacement,
// velocity and acceleration (concatenated in this order).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
// Velocity boundary condition code //

#include <cmath>
#include <filesystem>
#include <iostream>
#include <ostream>
#include <sstream>
#include <string>

#include "dumpable_iohelper.hh"

#include "dumper_text.hh"
#include "dumper_variable.hh"
#include "solid_mechanics_model.hh"

#include "aka_common.hh"
#include "mesh_utils.hh"
#include "ntn_base_contact.hh"
#include "ntn_contact_solvercallback.hh"
#include "ntn_initiation_function.hh"
#include <iomanip>              // akantu-rs
#include "akantu_rs_sha256.hh" // akantu-rs

using namespace akantu;
// akantu-rs: SHA-256 of the raw little-endian bytes of the arrays, concatenated
static std::string state_sha(std::initializer_list<const Array<Real> *> arrays) {
  akantu_rs::Sha256 h;
  for (const auto * a : arrays) {
    h.update(a->data(), sizeof(Real) * a->size() * a->getNbComponent());
  }
  return h.hex();
}

/* ------------------------------------------------------------------------ */
/* Main                                                                     */
/* ------------------------------------------------------------------------ */
int main(int /*argc*/, char * /*argv*/[]) {
  // akantu-rs: D-050 pin of Eigen's cache sizes (Eigen comes with the Akantu headers)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (not akantu_rs::Sha256::self_test()) { // akantu-rs
    std::cerr << "SHA-256 self-test failed\n"; // akantu-rs
    return 3;                                  // akantu-rs
  }                                            // akantu-rs

  // TODO read this from input file
  std::stringstream output_folder;
  output_folder << "steady_state";

  getStaticParser().parse("ras_ss.in");
  const ParserSection & data = getUserParser();
  UInt spatial_dimension = data.getParameter("spatial_dimension");
  UInt dump_every = data.getParameter("dump_every");
  std::unique_ptr<Mesh> mesh;
  std::unique_ptr<SolidMechanicsModel> model;
  std::unique_ptr<NTNContactSolverCallback> solver_ntn;
  mesh = std::make_unique<Mesh>(spatial_dimension);
  mesh->read("ntn_test_ras.msh");

  mesh->makePeriodic(_x, "slider_left", "slider_right");
  mesh->makePeriodic(_x, "base_left", "base_right");

  model = std::make_unique<SolidMechanicsModel>(*mesh);

  Real time_step_factor = data.getParameter("time_step_factor");

  Int normal_dir = 1;

  solver_ntn = std::make_unique<NTNContactSolverCallback>(
      *model, "slider_bottom", "base_top", normal_dir, time_step_factor);

  const auto & mat = model->getMaterial("slider");

  Real shear_vel = data.getParameter("shear_velocity");
  Vector<Real> trac_top = data.getParameter("top_traction");
  Vector<Real> trac_bottom = data.getParameter("bot_traction");

  model->setBaseName(output_folder.str());
  model->addDumpField("blocked_dofs");
  model->addDumpField("mass");
  model->addDumpFieldVector("velocity");
  model->addDumpFieldVector("acceleration");
  model->addDumpFieldVector("displacement");
  model->addDumpFieldVector("internal_force");
  model->addDumpFieldVector("external_force");

  // Static analytical solution
  Real fss = data.getParameter("fss");
  Real E = mat.getParam("E");
  Real nu = mat.getParam("nu");
  Real shear_modulus = E / (2. * (1. + nu));
  Real normal_strain_applied = trac_top(1) / E - nu * nu * trac_top(1) / E;

  Array<Real> & displacement = model->getDisplacement();
  Array<Real> & position = mesh->getNodes();
  UInt nb_nodes = model->getFEEngine().getMesh().getNbNodes();

  for (UInt n = 0; n < nb_nodes; ++n) {
    displacement(n, 0) = fss * -trac_top(1) / (shear_modulus)*position(n, 1);
    displacement(n, 1) = normal_strain_applied * position(n, 1);
  }

  // Set boundary conditions for dynamic simulation
  model->applyBC(BC::Neumann::FromTraction(trac_top), "slider_top");
  model->applyBC(BC::Neumann::FromTraction(trac_bottom), "base_bottom");

  ///// Set to steady state
  const auto & slider_nodes =
      mesh->getElementGroup("slider").getNodeGroup().getNodes();
  const auto & base_nodes =
      mesh->getElementGroup("base").getNodeGroup().getNodes();

  // Specify initial nodal velocity
  auto & velo = model->getVelocity();
  auto & increment = model->getIncrement();
  auto friction = solver_ntn->getFriction();
  auto dt = model->getTimeStep();

  for (auto n : slider_nodes) {
    velo(n, _x) = 0.5 * shear_vel;
    increment(n, _x) = 0.5 * shear_vel * dt;
  }
  for (auto n : base_nodes) {
    velo(n, _x) = -0.5 * shear_vel;
    increment(n, _x) = -0.5 * shear_vel * dt;
  }

  auto contact = solver_ntn->getContact();

  auto vel_it =
      make_view(model->getVelocity(), model->getSpatialDimension()).begin();
  auto & slip_velocity = friction->getSlipVelocity();
  auto & slip_velocity_norm = friction->getSlipVelocityNorm();
  auto & is_sticking = friction->getIsSticking();
  Real phi = friction->get("friction_state");

  for (auto && [n, master, slave, slip_vel, slip_vel_n, is_sticking] :
       enumerate(contact->getMasters(), contact->getSlaves(),
                 make_view(slip_velocity, slip_velocity.getNbComponent()),
                 slip_velocity_norm, is_sticking)) {

    is_sticking = false;

    friction->updateFrictionState(n, phi);
  }
  //////

  // Time of the simulation
  Real stable_time_step = model->getStableTimeStep();
  Real time_step = stable_time_step * time_step_factor;
  model->setTimeStep(time_step);
  UInt nb_steps = data.getParameter("nb_steps");

  model->dump();

  std::ofstream energies;
  auto file_name = std::filesystem::path(output_folder.str());
  file_name.replace_extension("csv");
  file_name = std::string("friction-energies-") + file_name.string();
  energies.open(file_name.c_str(), std::ofstream::out | std::ofstream::trunc);

  energies << "time,ekin,epot,work,econ,efri,tot" << std::endl;
  std::ofstream energies_full("energies_full.csv");                      // akantu-rs
  energies_full << std::setprecision(17)                                  // akantu-rs
                << "step,time,ekin,epot,work,econ,efri,tot,time_step,sha256(displacement|velocity|acceleration)\n"; // akantu-rs

  auto einit = 0.;

  for (UInt s = 0; s < nb_steps; ++s) {
    // Apply velocity
    UInt nb_nodes = model->getFEEngine().getMesh().getNbNodes();
    Array<Real> & position = mesh->getNodes();
    Array<Real> & velo = model->getVelocity();
    const Vector<Real> & upperBounds = mesh->getUpperBounds();
    const Vector<Real> & lowerBounds = mesh->getLowerBounds();
    Real top = upperBounds(1);
    Real bottom = lowerBounds(1);
    Real stable_time_step = model->getStableTimeStep();
    Real time_step = stable_time_step * time_step_factor;
    Real disp_incr = shear_vel * time_step;
    Array<Real> & displacement = model->getDisplacement();
    Array<bool> & blocked = model->getBlockedDOFs();
    for (UInt n = 0; n < nb_nodes; ++n) {
      if (std::abs(position(n, 1) - top) < 1e-6) {
        velo(n, _x) = 0.5 * shear_vel;
      }
      if (std::abs(position(n, 1) - bottom) < 1e-6) {
        velo(n, _x) = -0.5 * shear_vel;
      }
    }

    for (UInt n = 0; n < nb_nodes; ++n) {
      if (std::abs(position(n, 1) - top) < 1e-6) {
        displacement(n, 0) += 0.5 * disp_incr;
        blocked(n, 0) = true;
      }
      if (std::abs(position(n, 1) - bottom) < 1e-6) {
        displacement(n, 0) += -0.5 * disp_incr;
        blocked(n, 0) = true;
      }
    }

    model->solveStep(*solver_ntn, "explicit_lumped");

    auto ekin = model->getEnergy("kinetic");
    auto epot = model->getEnergy("potential");
    auto work = model->getEnergy("external work new");
    auto econ = solver_ntn->getExternalWork();
    if (s == 0) {
      einit = ekin + epot - (work + econ[0] + econ[1]);
    }
    energies << s * time_step << "," << ekin << "," << epot << "," << work
             << "," << econ[0] << "," << econ[1] << ","
             << ekin + epot - (work + econ[0] + econ[1]) - einit << std::endl;

    energies_full << s << "," << s * time_step << "," << ekin << "," << epot // akantu-rs
                  << "," << work << "," << econ[0] << "," << econ[1] << ","  // akantu-rs
                  << ekin + epot - (work + econ[0] + econ[1]) - einit << ","  // akantu-rs
                  << time_step << ","                                         // akantu-rs
                  << state_sha({&model->getDisplacement(), &model->getVelocity(), // akantu-rs
                                &model->getAcceleration()}) << "\n";          // akantu-rs
    if (s % dump_every == 0) {
      model->dump();
    }

    if (s == 10) {
      Real left = lowerBounds(0);
      Real right = upperBounds(0);
      Real contact_l = right - left;
      Real value;
      Real amplitude = 0.01;
      auto & cur_pos = contact->getModel().getCurrentPosition();
      auto & slaves = contact->getSlaves();

      auto & friction_state = friction->getState();
      UInt nb_contact_nodes = contact->getNbContactNodes();
      for (UInt n = 0; n < nb_contact_nodes; ++n) {
        UInt slave = slaves(n);
        value = friction_state(n) *
                (1 + amplitude * sin(2 * M_PI / contact_l * cur_pos(slave)));

        friction->updateFrictionState(n, value);
      }
    }
  }

  return EXIT_SUCCESS;
}
