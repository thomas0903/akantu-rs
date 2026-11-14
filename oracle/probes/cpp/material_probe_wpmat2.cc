// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): extra translation unit of material_probe that registers
// the material keyword `neohookean_kernel` — upstream's MaterialNeohookean
// (src/model/solid_mechanics/materials/material_finite_deformation/
// material_neohookean{.hh,.cc,_inline_impl.hh}) with computeStress,
// computeTangentModuli and computePotentialEnergy re-routed to its own
// point kernels, skipping only the parent calls that reach
// AKANTU_TO_IMPLEMENT (UD-026). Nothing of the library is rebuilt: the
// kernels are header templates compiled here with the oracle's flags.
//
// What the fixtures then record is upstream's *kernel* behaviour:
//   * stress: S = mu I + (lambda ln J - mu) C^-1 (J = det F sqrt(C33)) into
//     piola_kirchhoff_2, after the plane-stress C33 Newton-Raphson
//     (computeThirdAxisDeformation, tolerance 1e-5) when Plane_Stress;
//   * tangent: the non-condensed Belytschko tangent;
//   * energy: upstream's (defective, UD-057) energy kernel.
// The port's documented law differs on purpose for the energy and the
// plane-stress tangent; the Rust test compares each part accordingly.

#include <aka_common.hh>
#include <material_neohookean.hh>
#include <solid_mechanics_model.hh>

namespace akantu {
namespace {

template <Int dim>
class MaterialNeohookeanKernel : public MaterialNeohookean<dim> {
  using Parent = MaterialNeohookean<dim>;

public:
  MaterialNeohookeanKernel(SolidMechanicsModel & model, const ID & id = "")
      : Parent(model, id) {}

  /// PlaneStressToolbox<dim>::computeStress without the aborting
  /// Material::computeStress, then MaterialNeohookean's stress loop.
  void computeStress(ElementType el_type, GhostType ghost_type) override {
    if constexpr (dim == 2) {
      if (this->plane_stress) {
        this->computeThirdAxisDeformation(el_type, ghost_type);
      }
    }
    for (auto && args : this->getArguments(el_type, ghost_type)) {
      this->computeStressOnQuad(args);
    }
  }

  /// MaterialNeohookean::computeTangentModuli without Parent::computeStress.
  void computeTangentModuli(ElementType el_type, Array<Real> & tangent_matrix,
                            GhostType ghost_type) override {
    for (auto && args :
         Parent::getArgumentsTangent(tangent_matrix, el_type, ghost_type)) {
      this->computeTangentModuliOnQuad(args);
    }
  }

  /// MaterialNeohookean::computePotentialEnergy without
  /// Parent::computePotentialEnergy.
  void computePotentialEnergy(ElementType el_type) override {
    for (auto && [args, epot] :
         zip(Parent::getArguments(el_type, _not_ghost),
             this->potential_energy(el_type, _not_ghost))) {
      this->computePotentialEnergyOnQuad(args, epot);
    }
  }
};

[[maybe_unused]] const bool neohookean_kernel_registered =
    instantiateMaterial<MaterialNeohookeanKernel>("neohookean_kernel");

} // namespace
} // namespace akantu
