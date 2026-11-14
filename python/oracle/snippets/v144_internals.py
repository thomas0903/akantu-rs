# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet:
# the elastic law's own internals (MaterialThermal's delta_T,
# sigma_th, epsilon_th) read and written from inside the overrides of a
# MaterialLinearElastic2D subclass (the lent state:
# akantu::solid::DerivedLawState::own_real_internals), both modules.
# Inputs (work dir): bar_cm.msh.
import numpy as np

import akantu as aka
from _out import out

INPUT = """
model solid_mechanics_model [
  material v144_int [
    name = bar
    rho = 1.
    E = 1.
    nu = .3
  ]
]
"""
seen = {}


class Internals(aka.MaterialLinearElastic2D):
    def initMaterial(self):
        super().initMaterial()
        for name in ("delta_T", "sigma_th", "epsilon_th"):
            for t in self.getElementFilter().elementTypes():
                a = self.getInternalReal(name)(t)
                seen[f"init.{name}"] = list(a.shape)
                if name == "delta_T":
                    a[:] = 0.0

    def computeStress(self, el_type, ghost_type):
        if "stress.delta_T" not in seen and ghost_type == aka._not_ghost:
            seen["stress.delta_T"] = float(np.abs(self.getInternalReal("delta_T")(el_type)).max())
        lam, mu = self.getReal("lambda"), self.getReal("mu")
        g = self.getGradU(el_type, ghost_type)
        n = g.shape[0]
        g = g.reshape((n, 2, 2))
        eps = 0.5 * (g + np.einsum("aij->aji", g))
        tr = np.einsum("aii->a", g)
        self.getStress(el_type, ghost_type).reshape((n, 2, 2))[:] = (
            np.einsum("a,ij->aij", tr, lam * np.eye(2)) + 2.0 * mu * eps)


aka.MaterialFactory.getInstance().registerAllocator(
    "v144_int", lambda dim, opt, model, _id: Internals(model, _id))
open("v144_int.dat", "w").write(INPUT)
aka.parseInput("v144_int.dat")
mesh = aka.Mesh(2)
mesh.read("bar_cm.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._explicit_lumped_mass)
model.setTimeStep(model.getStableTimeStep() * 0.5)
model.solveStep()
for k in sorted(seen):
    out(k, seen[k])
