// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe (a verification audit gap):
// what upstream PhaseFieldModel does with the two explicit analysis methods
// (src/model/phase_field/phase_field_model.cc:107-188: getDefaultSolverID,
// getDefaultSolverOptions, initSolver). Error-path evidence only, no
// fixture: for each method it prints whether initFull, setTimeStep and one
// solveStep succeed, the exception text otherwise, and the nodal damage
// after a successful step.
//
// Usage: explicit_branches_probe <material.dat> <mesh.msh>
// (upstream test/test_model/test_phase_field_model/material_coupling.dat and
// test_one_element.msh).

#include <aka_common.hh>
#include <mesh.hh>
#include <phase_field_model.hh>

#include <exception>
#include <iostream>
#include <string>

using namespace akantu;

namespace {
template <class F> bool step(const std::string & what, F && f) {
  try {
    f();
    std::cout << "  " << what << ": ok\n";
    return true;
  } catch (std::exception & e) {
    std::cout << "  " << what << ": exception: " << e.what() << "\n";
    return false;
  }
}
} // namespace

int main(int argc, char * argv[]) {
  if (argc < 3) {
    std::cerr << "usage: " << argv[0] << " <material.dat> <mesh.msh>\n";
    return 2;
  }
  initialize(argv[1], argc, argv);
  for (auto method : {_explicit_lumped_mass, _explicit_consistent_mass}) {
    // one mesh per model: a second PhaseFieldModel on the same mesh (at the
    // same stack address) is refused by the mesh's event-handler manager
    // ("This event handler was already registered (priority: 94)")
    Mesh mesh(2);
    mesh.read(argv[2]);
    std::cout << "method " << (method == _explicit_lumped_mass ? "_explicit_lumped_mass"
                                                               : "_explicit_consistent_mass")
              << "\n";
    PhaseFieldModel model(mesh, 2, "phase_field_model");
    if (not step("initFull", [&] { model.initFull(_analysis_method = method); })) {
      continue;
    }
    step("setTimeStep(1e-3)", [&] { model.setTimeStep(1e-3); });
    if (step("solveStep", [&] { model.solveStep(); })) {
      const auto & damage = model.getDamage();
      std::cout << "  damage:";
      for (Int i = 0; i < damage.size(); ++i) {
        std::cout << " " << damage(i);
      }
      std::cout << "\n";
    }
  }
  finalize();
  return 0;
}
