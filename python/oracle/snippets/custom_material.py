# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 2) oracle snippet: upstream
# examples/python/solid_mechanics_model/custom-material/custom-material.py
# (a Python Material registered with MaterialFactory.registerAllocator,
# parsed from material.dat, explicit dynamics), 40 steps instead of 250.
import numpy as np

import akantu as aka
from _out import out


class LocalElastic(aka.Material):
    def __init__(self, model, _id):
        super().__init__(model, _id)
        super().registerParamReal(
            "E", aka._pat_readable | aka._pat_parsable, "Youngs modulus"
        )
        super().registerParamReal(
            "nu", aka._pat_readable | aka._pat_parsable, "Poisson ratio"
        )

    def initMaterial(self):
        nu = self.getReal("nu")
        E = self.getReal("E")
        self.mu = E / (2 * (1 + nu))
        self.lame_lambda = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
        self.lame_mu = E / (2.0 * (1.0 + nu))
        super().initMaterial()

    def getPushWaveSpeed(self, element):
        rho = self.getReal("rho")
        return np.sqrt((self.lame_lambda + 2 * self.lame_mu) / rho)

    @staticmethod
    def computeEpsilon(grad_u):
        return 0.5 * (grad_u + np.einsum("aij->aji", grad_u))

    def computeStress(self, el_type, ghost_type):
        grad_u = self.getGradU(el_type, ghost_type)
        sigma = self.getStress(el_type, ghost_type)
        n_quads = grad_u.shape[0]
        grad_u = grad_u.reshape((n_quads, 2, 2))
        epsilon = self.computeEpsilon(grad_u)
        sigma = sigma.reshape((n_quads, 2, 2))
        trace = np.einsum("aii->a", grad_u)
        sigma[:, :, :] = (
            np.einsum("a,ij->aij", trace, self.lame_lambda * np.eye(2))
            + 2.0 * self.lame_mu * epsilon
        )

    def computeTangentModuli(self, el_type, tangent_matrix, ghost_type):
        n_quads = tangent_matrix.shape[0]
        tangent = tangent_matrix.reshape(n_quads, 3, 3)
        Miiii = self.lame_lambda + 2 * self.lame_mu
        Miijj = self.lame_lambda
        Mijij = self.lame_mu
        tangent[:, 0, 0] = Miiii
        tangent[:, 1, 1] = Miiii
        tangent[:, 0, 1] = Miijj
        tangent[:, 1, 0] = Miijj
        tangent[:, 2, 2] = Mijij

    def computePotentialEnergy(self, el_type):
        sigma = self.getStress(el_type)
        grad_u = self.getGradU(el_type)
        nquads = sigma.shape[0]
        stress = sigma.reshape(nquads, 2, 2)
        grad_u = grad_u.reshape((nquads, 2, 2))
        epsilon = self.computeEpsilon(grad_u)
        energy_density = self.getPotentialEnergy(el_type)
        energy_density[:, 0] = 0.5 * np.einsum("aij,aij->a", stress, epsilon)


def allocator(_dim, unused, model, _id):
    return LocalElastic(model, _id)


mat_factory = aka.MaterialFactory.getInstance()
mat_factory.registerAllocator("local_elastic", allocator)
names = mat_factory.getPossibleAllocators()
out("factory.has_local_elastic", "local_elastic" in names)
out("factory.has_elastic", "elastic" in names)
try:
    mat_factory.registerAllocator("elastic", allocator)
    out("factory.duplicate", "accepted")
except Exception as e:  # noqa: BLE001
    out("factory.duplicate", type(e).__name__)

aka.parseInput("material_cm.dat")
mesh = aka.Mesh(2)
mesh.read("bar_cm.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._explicit_lumped_mass)
model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
displacement = model.getDisplacement()
position = mesh.getNodes()
for i in range(0, mesh.getNbNodes()):
    x = position[i, 0] - 5.0
    L = 1
    k = 0.1 * 2 * np.pi * 3 / L
    displacement[i, 0] = 0.01 * np.sin(k * x) * np.exp(-(k * x) * (k * x) / (L * L))
mat = model.getMaterial(0)
out("mat.class", type(mat).__name__)
out("mat.E", mat.getReal("E"))
out("mat.rho", mat.getReal("rho"))
stable = model.getStableTimeStep()
out("stable_time_step", stable)
model.setTimeStep(stable * 0.64)
for step in range(40):
    model.solveStep()
    if step % 10 == 0:
        out(f"step{step}.epot", model.getEnergy("potential"))
        out(f"step{step}.ekin", model.getEnergy("kinetic"))
out("u", model.getDisplacement())
out("stress.t3", mat.getStress(aka._triangle_3))
