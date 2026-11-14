# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 5) oracle snippet: HeatTransferModel
# (explicit lumped capacity, no sparse solver), StructuralMechanicsModel
# (displacement written through its view, potential energy K u . u / 2,
# lumped-mass assembly (shape only, UD-005), materials; no linear solve: upstream's structural initFull
# takes only an analysis method, so the oracle would solve with MUMPS and
# the port with Eigen).

import akantu as aka
from _out import out


# --- heat transfer ---------------------------------------------------------------
aka.parseInput("heat_transfer_input.dat")
mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
model = aka.HeatTransferModel(mesh)
model.initFull(_analysis_method=aka._explicit_lumped_mass)
T = model.getTemperature()
b = model.getBlockedDOFs()
X = mesh.getNodes()
left, right = X[:, 0] < X[:, 0].min() + 1e-12, X[:, 0] > X[:, 0].max() - 1e-12
b[left | right, 0] = True
T[left, 0] = 1.0
T[right, 0] = 0.0
dt = model.getStableTimeStep()
out("heat.stable_dt", dt)
model.setTimeStep(0.8 * dt)
for _ in range(25):
    model.solveStep()
out("heat.T", model.getTemperature())
out("heat.nb_laws", model.getNbConstitutiveLaws())
law = model.getConstitutiveLaw(0)
out("heat.law", type(law).__name__)
for t in law.getInternalReal("∇u").elementTypes():
    out("heat.grad", law.getInternalReal("∇u")(t))
    out("heat.flux", law.getInternalReal("D∇u")(t))
out("heat.local_numbering", model.getConstitutiveLawLocalNumbering()(aka._quadrangle_8))
flat = model.flattenInternalReal("D∇u")
out("heat.flat_flux", flat(aka._quadrangle_8))

# --- structural ---------------------------------------------------------------------
smesh = aka.Mesh(2)
acc = aka.MeshAccessor(smesh)
acc.resizeNodes(3)
smesh.getNodes()[:] = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]
smesh.addConnectivityType(aka._bernoulli_beam_2)
acc.resizeConnectivity(2, aka._bernoulli_beam_2)
smesh.getConnectivity(aka._bernoulli_beam_2)[:] = [[0, 1], [1, 2]]
acc.makeReady()
smodel = aka.StructuralMechanicsModel(smesh)
mat = aka.StructuralMaterial()
mat.E, mat.A, mat.I, mat.rho = 1e9, 1.0, 1.0, 2.5
smodel.addMaterial(mat)
smodel.initFull(aka._static)
smodel.getDisplacement()[:] = [[0.0, 0.0, 0.0], [1e-3, 5e-3, 2e-3], [2e-3, 1e-2, 1e-3]]
out("struct.epot", smodel.getPotentialEnergy())
out("struct.energy", smodel.getEnergy("potential"))
smodel.assembleLumpedMassMatrix()
out("struct.has_lumped", smodel.hasLumpedMass())
# getLumpedMass(): upstream returns zeros (UD-005, `node2 = conn(0)`), the
# port the corrected m/2 per node: a documented difference, not compared
out("struct.lumped_shape", smodel.getLumpedMass().shape)
out("struct.nb_materials", smodel.getNbMaterials())
out("struct.element_material", smodel.getElementMaterial(aka._bernoulli_beam_2))
out("struct.material.E", smodel.getMaterial(0).E)
