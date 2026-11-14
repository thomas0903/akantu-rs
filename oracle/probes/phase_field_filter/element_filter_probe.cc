// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe (PF-011): upstream
// PhaseFieldElementFilter (src/model/phase_field/phase_field_element_filter.hh)
// on upstream's four-element mesh with a prescribed nodal damage whose
// interpolation straddles the thresholds inside some elements. Prints, per
// element, the law's damage at its quadrature points (IEEE-754 hex) and the
// filter's verdict ("unbroken if any quadrature point is below max_damage")
// for each threshold.
//
// Usage: element_filter_probe <material.dat> <mesh.msh>
// (upstream test/test_model/test_phase_field_model/material_coupling.dat and
// test_four_elements.msh).

#include <aka_common.hh>
#include <mesh.hh>
#include <phase_field_element_filter.hh>
#include <phase_field_model.hh>

#include <Eigen/Core>

#include <cinttypes>
#include <cstdint>
#include <cstdio>
#include <cstring>

using namespace akantu;

namespace {
// the element -> (law, local index) maps are protected members of upstream's
// ConstitutiveLawsHandler; a derived model re-exposes them (no behavior
// change: same constructor, same members)
struct ProbeModel : public PhaseFieldModel {
  using PhaseFieldModel::PhaseFieldModel;
  using PhaseFieldModel::getConstitutiveLawByElement;
  using PhaseFieldModel::getConstitutiveLawLocalNumbering;
};

std::uint64_t bits(double x) {
  std::uint64_t b;
  std::memcpy(&b, &x, sizeof b);
  return b;
}
} // namespace

int main(int argc, char * argv[]) {
  if (argc < 3) {
    std::fprintf(stderr, "usage: %s <material.dat> <mesh.msh>\n", argv[0]);
    return 2;
  }
  initialize(argv[1], argc, argv);
  // D-050 pin (no sparse solve here, recorded for provenance)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  std::fprintf(stderr, "[filter] eigen cache sizes l1=%td l2=%td l3=%td\n", Eigen::l1CacheSize(),
               Eigen::l2CacheSize(), Eigen::l3CacheSize());
  Mesh mesh(2);
  mesh.read(argv[2]);
  ProbeModel model(mesh, 2, "phase_field_model");
  model.initFull(_analysis_method = _static);
  // nodal damage by node index (the frozen mesh's numbering)
  const double nodal[] = {0.0, 0.25, 1.0, 0.95, 0.6, 0.999, 0.1, 0.5, 0.75};
  auto & damage = model.getDamage();
  for (Int n = 0; n < damage.size(); ++n) {
    damage(n) = nodal[n % 9];
    std::printf("NODE %" PRId64 " %016" PRIx64 "\n", static_cast<std::int64_t>(n), bits(damage(n)));
  }
  for (Int m = 0; m < model.getNbPhaseFields(); ++m) {
    model.getPhaseField(m).computeAllDrivingForces(_not_ghost);
  }
  const double thresholds[] = {0.3, 0.5, 0.8, 0.97, 0.999, 1.0};
  for (auto type : mesh.elementTypes(2, _not_ghost)) {
    auto nb_el = mesh.getNbElement(type, _not_ghost);
    for (Int e = 0; e < nb_el; ++e) {
      Element el{type, e, _not_ghost};
      auto law = model.getConstitutiveLawByElement()(el);
      auto local = model.getConstitutiveLawLocalNumbering()(el);
      const auto & qd = model.getPhaseField(law).getDamage(type, _not_ghost);
      auto nq = model.getFEEngine().getNbIntegrationPoints(type, _not_ghost);
      std::printf("QDAMAGE %s %" PRId64, std::to_string(type).c_str(), static_cast<std::int64_t>(e));
      for (Int q = 0; q < nq; ++q) {
        std::printf(" %016" PRIx64, bits(qd(local * nq + q)));
      }
      std::printf("\n");
      for (double t : thresholds) {
        PhaseFieldElementFilter filter(model, t);
        std::printf("FILTER %s %" PRId64 " %016" PRIx64 " %d\n", std::to_string(type).c_str(),
                    static_cast<std::int64_t>(e), bits(t), filter(el) ? 1 : 0);
      }
    }
  }
  finalize();
  return 0;
}
