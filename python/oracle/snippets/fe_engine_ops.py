# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 7, PY-057) oracle snippet: the FEEngine operators
# computeBtD, computeBtDB (orders 4 and 2), computeNtb, computeNtbN (with
# and without filter), the ElementTypeMapArray overloads of integrate and
# interpolateOnIntegrationPoints (maps initialised from the engine),
# assembleFieldLumped into a NumPy array and assembleFieldMatrix into a new
# matrix of the DOF manager with Python field functors (UD-836: see below).
import numpy as np

import akantu as aka
from _out import out

aka.parseInput("elastic.dat")
mesh = aka.Mesh(2)
mesh.read("test_four_elements.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(
    aka.ModelOptions(
        aka._implicit_dynamic,
        aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen),
    )
)
fe = model.getFEEngine()
t = aka._quadrangle_4
nq = fe.getNbIntegrationPoints(t)
ne = mesh.getNbElement(element_type=t)
X = mesh.getNodes()
n = ne * nq

u = np.column_stack([X[:, 0] ** 2 + 0.3 * X[:, 1], X[:, 0] * X[:, 1] - 0.7 * X[:, 1] ** 3])
grad = np.zeros((n, 4))
fe.gradientOnIntegrationPoints(u, grad, 2, t)
btd = np.zeros((n, 8))
fe.computeBtD(grad, btd, t)
out("fe.btd", btd)

voigt = np.array([[4.0, 1.0, 0.0], [1.0, 4.0, 0.0], [0.0, 0.0, 1.5]])
D4 = np.tile(voigt.ravel(), (n, 1)) * np.linspace(1.0, 2.0, n)[:, None]
btdb4 = np.zeros((n, 64))
fe.computeBtDB(D4, btdb4, 4, t)
out("fe.btdb4", btdb4)
D2 = np.tile(np.array([2.0, 0.5, 0.5, 1.0]), (n, 1))
btdb2 = np.zeros((n, 16))
fe.computeBtDB(D2, btdb2, 2, t)
out("fe.btdb2", btdb2)

b = np.column_stack([np.linspace(1.0, 2.0, n), np.linspace(-1.0, 1.0, n)])
ntb = np.zeros((n, 8))
fe.computeNtb(b, ntb, t)
out("fe.ntb", ntb)
ntbn = np.zeros((n, 16))
fe.computeNtbN(np.ascontiguousarray(b[:, :1]), ntbn, t)
out("fe.ntbn", ntbn)
filt = np.array([[1], [3]], dtype=np.int32)  # upstream's caster needs 2-D
ntbf = np.zeros((2 * nq, 8))
fe.computeNtb(np.ascontiguousarray(b[: 2 * nq]), ntbf, t, filter_elements=filt)
out("fe.ntb_filtered", ntbf)

# ElementTypeMapArray overloads
fq = aka.ElementTypeMapArrayReal()
fq.initialize(fe, nb_component=2)
out("fe.etma_shape", fq(t).shape)
fq(t)[:] = b
intf = aka.ElementTypeMapArrayReal()
intf.initialize(mesh, nb_component=2, with_nb_element=True)
fe.integrate(fq, intf)
out("fe.integrate_etma", intf(t))
ref = np.zeros((ne, 2))
fe.integrate(b, ref, 2, t)
out("fe.integrate_array", ref)
uq = aka.ElementTypeMapArrayReal()
uq.initialize(fe, nb_component=2)
fe.interpolateOnIntegrationPoints(u, uq)
out("fe.interpolate_etma", uq(t))

# field assembly with Python functors: upstream hands the functor a copy of
# the matrix (pybind11 casts an lvalue Eigen reference argument by copy), so
# its writes are lost and upstream assembles zeros (UD-836); compared here:
# the matrices handed to the functor (shape, layout, element order) and the
# model's own lumped / consistent masses (the port's functor results equal
# them bitwise: python/tests/test_fe_engine_ops.py::test_field_assembly)
info = []


def rho(mat, element):
    info.append(
        (mat.shape, bool(mat.flags["F_CONTIGUOUS"]), bool(mat.flags["WRITEABLE"]),
         bool(np.all(mat == 0.0)), repr(element))
    )
    mat[:, :] = 2500.0


dof = model.getDOFManager()
lumped = np.zeros((mesh.getNbNodes(), 2))
fe.assembleFieldLumped(rho, lumped, dof, t)
m2 = dof.getNewMatrix("M2", "M")
m2.zero()
fe.assembleFieldMatrix(rho, "M2", "displacement", dof, t)
out("fe.functor_calls", repr(info))
model.assembleMassLumped()
out("fe.model_lumped_mass", model.getMass())
model.assembleMass()
mass = dof.getMatrix("M")
size = mass.size()
out("fe.model_mass_matrix", np.array([[mass(i, j) for j in range(size)] for i in range(size)]))
