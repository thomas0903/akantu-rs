# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet: small SolidMechanicsModel static solve
# with Python BCs and functors (python/py_solid_mechanics_model.cc,
# py_boundary_conditions.cc, test/test_python_interface/test_pybind.py).
# The sparse solver is pinned to Eigen (the oracle's auto solver is MUMPS).
import numpy as np

import akantu as aka
from _out import out

aka.parseInput("elastic.dat")
mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
model = aka.SolidMechanicsModel(mesh)
opts = aka.ModelOptions(
    aka._static,
    aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen),
)
model.initFull(opts)


class Fixed(aka.DirichletFunctor):
    def __init__(self, value, axis):
        super().__init__(axis)
        self.value = value
        self.axis = int(axis)

    def __call__(self, node, flags, primal, coord):
        primal[self.axis] = self.value
        flags[self.axis] = True


class Traction(aka.NeumannFunctor):
    def __call__(self, quad_point, dual, coord, normals):
        dual[:] = np.dot(np.array([[0.0, 1e5], [1e5, 2e5]]), normals)


X = mesh.getNodes()
blocked = model.getBlockedDOFs()
left = X[:, 0] < 1e-12
right = X[:, 0] > X[:, 0].max() - 1e-12
blocked[left, 0] = True
model.applyBC(Fixed(0.0, aka._y), "edge")  # left edge, y (Python functor)
model.applyBC(Traction(), "coh")  # interior line (Python functor)
fext = model.getExternalForce()
fext[right, 1] += -10.0  # nodal load written through the view
out("fext", model.getExternalForce())
solver = model.getNonLinearSolver()
solver.set("max_iterations", 4)
solver.set("threshold", 1e-10)
solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)
model.solveStep()
out("nb_iterations", solver.getInt("nb_iterations"))
out("u", model.getDisplacement())
out("fint", model.getInternalForce())
mat = model.getMaterial(0)
out("stress.q8", mat.getStress(aka._quadrangle_8))
out("grad_u.q8", mat.getGradU(aka._quadrangle_8))
out("epot", model.getEnergy("potential"))
# getEnergy("potential", group) integrates uninitialised memory upstream
# (UD-032); not compared.
out("E", mat.getReal("E"))
out("blocked.sum", int(model.getBlockedDOFs().sum()))
out("matbyel", model.getMaterialByElement()(aka._quadrangle_8))
