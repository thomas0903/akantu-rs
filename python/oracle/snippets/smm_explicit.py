# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet: explicit dynamics through the Python
# API (time step, solveStep loop, energies; examples/python/
# solid_mechanics_model/dynamics style).
import numpy as np

import akantu as aka
from _out import out

aka.parseInput("elastic.dat")
mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._explicit_lumped_mass)
dt = model.getStableTimeStep()
out("stable_dt", dt)
model.setTimeStep(0.8 * dt)
X = mesh.getNodes()
blocked = model.getBlockedDOFs()
blocked[X[:, 0] < 1e-12, :] = True
v = model.getVelocity()
v[:, 1] = 1e-2 * X[:, 0] / X[:, 0].max()
ek = []
for s in range(20):
    model.solveStep()
    ek.append(model.getEnergy("kinetic"))
out("ekin", np.array(ek))
out("epot", model.getEnergy("potential"))
out("u", model.getDisplacement())
out("mass", model.getMass())
