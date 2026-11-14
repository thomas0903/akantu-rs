// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle variant of the upstream example
// examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/cohesive_extrinsic_ig_tg.cc (reference
// 35a851780d0b9471d258ed53a003ae98aaa95ea0), copied verbatim except the lines
// marked `akantu-rs:` (= cohesive_extrinsic_ig_tg_full.cc with a different
// initial velocity). Changes: Eigen's cache sizes are pinned to its defaults
// (decision D-050: host-independent blocking); `energy_full.csv` records,
// after every step, the number of cohesive elements and the dissipated,
// reversible, potential and kinetic energies with 17 significant digits; the
// initial velocity is `0.05 x` instead of `loading_rate x = 0.1 x`
// (the blocked side DOFs then start at ±0.05, so the velocity the
// `Velocity` functor writes, ±VI = ±0.1, changes the state; in the upstream
// example it rewrites the initial value bitwise).
// Built by oracle/probes/examples/CMakeLists.txt, run by
// oracle/probes/examples/run-in-container.sh.
/**
 * Copyright (©) 2014-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
 * Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
 *
 * This file is part of Akantu
 *
 * Akantu is free software: you can redistribute it and/or modify it under the
 * terms of the GNU Lesser General Public License as published by the Free
 * Software Foundation, either version 3 of the License, or (at your option) any
 * later version.
 *
 * Akantu is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
 * A PARTICULAR PURPOSE. See the GNU Lesser General Public License for more
 * details.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with Akantu. If not, see <http://www.gnu.org/licenses/>.
 */

/* -------------------------------------------------------------------------- */
#include "solid_mechanics_model_cohesive.hh"
/* -------------------------------------------------------------------------- */
#include <fstream>  // akantu-rs
#include <iomanip>  // akantu-rs
#include <iostream>
/* -------------------------------------------------------------------------- */

using namespace akantu;

/* -------------------------------------------------------------------------- */
class Velocity : public BC::Dirichlet::DirichletFunctor {
public:
  explicit Velocity(SolidMechanicsModel & model, Real vel, BC::Axis ax = _x)
      : DirichletFunctor(ax), model(model), vel(vel) {
    disp = vel * model.getTimeStep();
  }

public:
  inline void operator()(Idx node, VectorProxy<bool> & /*flags*/,
                         VectorProxy<Real> & disp,
                         const VectorProxy<const Real> & coord) override {
    Real sign = std::signbit(coord(axis)) ? -1. : 1.;
    disp(axis) += sign * this->disp;
    model.getVelocity()(node, axis) = sign * vel;
  }

private:
  SolidMechanicsModel & model;
  Real vel, disp;
};

/* -------------------------------------------------------------------------- */
int main(int argc, char * argv[]) {
  // akantu-rs: D-050, Eigen's default cache sizes (host-independent blocking)
  Eigen::setCpuCacheSizes(16384, 524288, 524288); // akantu-rs
  initialize("material.dat", argc, argv);

  const Int spatial_dimension = 2;
  const Int max_steps = 1000;

  Mesh mesh(spatial_dimension);
  mesh.read("square.msh");

  SolidMechanicsModelCohesive model(mesh);
  MaterialCohesiveRules rules{{{"btop", "bbottom"}, "tg_cohesive"},
                              {{"btop", "btop"}, "ig_cohesive"},
                              {{"bbottom", "bbottom"}, "ig_cohesive"}};

  /// model initialization
  auto cohesive_material_selector =
      std::make_shared<MaterialCohesiveRulesSelector>(model, rules);
  auto bulk_material_selector =
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names",
                                                              model);
  auto && current_selector = model.getMaterialSelector();

  cohesive_material_selector->setFallback(bulk_material_selector);
  bulk_material_selector->setFallback(current_selector);

  model.setMaterialSelector(cohesive_material_selector);

  model.initFull(_analysis_method = _explicit_lumped_mass,
                 _is_extrinsic = true);

  Real time_step = model.getStableTimeStep() * 0.05;
  model.setTimeStep(time_step);
  std::cout << "Time step: " << time_step << "\n";

  model.assembleMassLumped();

  auto & position = mesh.getNodes();
  auto & velocity = model.getVelocity();

  model.applyBC(BC::Dirichlet::FlagOnly(_y), "top");
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "bottom");

  model.applyBC(BC::Dirichlet::FlagOnly(_x), "left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "right");

  model.setBaseName("extrinsic");
  model.addDumpFieldVector("displacement");
  model.addDumpField("velocity");
  model.addDumpField("acceleration");
  model.addDumpField("internal_force");
  model.addDumpField("stress");
  model.addDumpField("grad_u");
  model.addDumpField("material_index");
  model.dump();

  /// initial conditions
  Real loading_rate = 0.1;
  // bar_height  = 2
  Real VI = loading_rate * 2 * 0.5;
  Real initial_rate = 0.05; // akantu-rs: initial velocity ≠ the functor's
  for (auto && [pos, vel] : zip(make_view(position, spatial_dimension),
                                make_view(velocity, spatial_dimension))) {
    vel = initial_rate * pos; // akantu-rs: upstream `loading_rate * pos`
  }

  model.dump();

  Velocity vely(model, VI, _y);
  Velocity velx(model, VI, _x);

  std::ofstream energy_full("energy_full.csv"); // akantu-rs
  energy_full << std::setprecision(17)          // akantu-rs
              << "step,nb_cohesive,dissipated,reversible,potential,kinetic\n"; // akantu-rs
  /// Main loop
  for (Int s = 1; s <= max_steps; ++s) {

    model.applyBC(vely, "top");
    model.applyBC(vely, "bottom");

    model.applyBC(velx, "left");
    model.applyBC(velx, "right");

    model.checkCohesiveStress();

    model.solveStep();
    // akantu-rs: 17-digit energy record of the step
    energy_full << s << ","                                             // akantu-rs
                << mesh.getNbElement(spatial_dimension, _not_ghost, _ek_cohesive) << "," // akantu-rs
                << model.getEnergy("dissipated") << ","                    // akantu-rs
                << model.getEnergy("reversible") << ","                    // akantu-rs
                << model.getEnergy("potential") << ","                     // akantu-rs
                << model.getEnergy("kinetic") << "\n";                    // akantu-rs

    if (s % 10 == 0) {
      model.dump();
      std::cout << "passing step " << s << "/" << max_steps << "\n";
    }
  }

  return 0;
}
