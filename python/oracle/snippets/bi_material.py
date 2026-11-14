# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 2) oracle snippet: upstream
# examples/python/solid_mechanics_model/custom-material/bi-material.py
# (registerCustomMaterials, initFull with a materials dict, runtime
# parameters and internals, getModel().getFEEngine()
# .computeIntegrationPointsCoordinates(internal, getElementFilter()), a
# static solve with the Python tangent), without the dumps.
import numpy as np

import akantu as aka
from _out import out


class LocalElastic(aka.Material):
    def __init__(self, model, _id):
        super().__init__(model, _id)
        super().registerParamReal(
            "E", aka._pat_readable | aka._pat_parsmod, "Youngs modulus"
        )
        super().registerParamReal(
            "nu", aka._pat_readable | aka._pat_parsmod, "Poisson ratio"
        )
        self.factor = super().registerInternalReal("factor", 1)
        self.quad_coords = super().registerInternalReal("quad_coordinates", 2)

    def initMaterial(self):
        nu = self.getReal("nu")
        E = self.getReal("E")
        self.mu = E / (2 * (1 + nu))
        self.lame_lambda = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
        self.lame_mu = E / (2.0 * (1.0 + nu))
        super().initMaterial()
        model = self.getModel()
        model.getFEEngine().computeIntegrationPointsCoordinates(
            self.quad_coords, self.getElementFilter()
        )
        for elem_type in self.factor.elementTypes():
            factor = self.factor(elem_type)
            coords = self.quad_coords(elem_type)
            factor[:] = 1.0
            factor[coords[:, 1] < 0.5] = 0.5

    @staticmethod
    def computeEpsilon(grad_u):
        return 0.5 * (grad_u + np.einsum("aij->aji", grad_u))

    def computeStress(self, el_type, ghost_type):
        grad_u = self.getGradU(el_type, ghost_type)
        sigma = self.getStress(el_type, ghost_type)
        n_quads = grad_u.shape[0]
        grad_u = grad_u.reshape((n_quads, 2, 2))
        factor = self.getInternalReal("factor")(el_type, ghost_type).reshape(
            n_quads
        )
        epsilon = self.computeEpsilon(grad_u)
        sigma = sigma.reshape((n_quads, 2, 2))
        trace = np.einsum("aii->a", grad_u)
        sigma[:, :, :] = (
            np.einsum("a,ij->aij", trace, self.lame_lambda * np.eye(2))
            + 2.0 * self.lame_mu * epsilon
        )
        sigma[:, :, :] = np.einsum("aij, a->aij", sigma, factor)

    def computeTangentModuli(self, el_type, tangent_matrix, ghost_type):
        n_quads = tangent_matrix.shape[0]
        tangent = tangent_matrix.reshape(n_quads, 3, 3)
        factor = self.getInternalReal("factor")(el_type, ghost_type).reshape(
            n_quads
        )
        Miiii = self.lame_lambda + 2 * self.lame_mu
        Miijj = self.lame_lambda
        Mijij = self.lame_mu
        tangent[:, 0, 0] = Miiii
        tangent[:, 1, 1] = Miiii
        tangent[:, 0, 1] = Miijj
        tangent[:, 1, 0] = Miijj
        tangent[:, 2, 2] = Mijij
        tangent[:, :, :] = np.einsum("aij, a->aij", tangent, factor)

    def computePotentialEnergy(self, el_type):
        sigma = self.getStress(el_type)
        grad_u = self.getGradU(el_type)
        nquads = sigma.shape[0]
        stress = sigma.reshape(nquads, 2, 2)
        grad_u = grad_u.reshape((nquads, 2, 2))
        epsilon = self.computeEpsilon(grad_u)
        energy_density = self.getPotentialEnergy(el_type)
        energy_density[:, 0] = 0.5 * np.einsum("aij,aij->a", stress, epsilon)


def applyBC(model):
    nbNodes = model.getMesh().getNbNodes()
    position = model.getMesh().getNodes()
    displacement = model.getDisplacement()
    blocked_dofs = model.getBlockedDOFs()
    width = 1.0
    height = 1.0
    epsilon = 1e-8
    for node in range(0, nbNodes):
        if (np.abs(position[node, 0]) < epsilon) or (
            np.abs(position[node, 0] - width) < epsilon
        ):
            blocked_dofs[node, 0] = True
            displacement[node, 0] = 0 * position[node, 0] + 0.0
        if np.abs(position[node, 1]) < epsilon:
            blocked_dofs[node, 1] = True
            displacement[node, 1] = -1.0
        if np.abs(position[node, 1] - height) < epsilon:
            blocked_dofs[node, 1] = True
            displacement[node, 1] = 1.0


aka.registerCustomMaterials({"local_elastic": LocalElastic})
mesh = aka.Mesh(2)
mesh.read("square.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(
    {"fictive": {"type": "local_elastic", "rho": 1.0, "E": 1.0, "nu": 0.3}},
    _analysis_method=aka._static,
)
mat = model.getMaterial(0)
out("mat.is_python", isinstance(mat, LocalElastic))
out("factor", mat.factor(aka._triangle_3))
out("quad_coords", mat.quad_coords(aka._triangle_3))
out("filter", mat.getElementFilter()(aka._triangle_3))
out("factor.id", mat.getInternalReal("factor").getID())
out("factor.types", [str(t) for t in mat.factor.elementTypes()])
# initFull's dict form takes no solver options: the oracle solves with its
# auto sparse solver (MUMPS), the port with Eigen — the solve outputs below
# agree to rounding only (bi_material_eigen.py pins Eigen on both sides)
solver = model.getNonLinearSolver()
solver.set("max_iterations", 2)
solver.set("threshold", 1e-3)
solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)
applyBC(model)
model.solveStep()
out("u", model.getDisplacement())
out("fint", model.getInternalForce())
out("stress", mat.getStress(aka._triangle_3))
out("epot", model.getEnergy("potential"))
