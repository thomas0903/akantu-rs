# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet: FEEngine basics through the Python API
# (python/py_fe_engine.cc; test/test_fe_engine/py_engine usage).
import numpy as np

import akantu as aka
from _out import out

aka.parseInput("elastic.dat")
mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._static)
fe = model.getFEEngine()
t = aka._quadrangle_8
nq = fe.getNbIntegrationPoints(t)
ne = mesh.getNbElement(element_type=t)
out("nq", nq)
X = mesh.getNodes()
u = np.zeros((X.shape[0], 2))
u[:, 0] = 0.01 * X[:, 0] + 0.02 * X[:, 1] ** 2
u[:, 1] = -0.03 * X[:, 0] * X[:, 1]
uq = np.zeros((ne * nq, 2))
fe.interpolateOnIntegrationPoints(u, uq, 2, t)
out("uq", uq)
grad = np.zeros((ne * nq, 4))
fe.gradientOnIntegrationPoints(u, grad, 2, t)
out("grad", grad)
intf = np.zeros((ne, 2))
fe.integrate(uq, intf, 2, t)
out("intf", intf)
out("int_scalar", fe.integrate(uq[:, :1].copy(), t))
out("shapes", fe.getShapes(t))
out("dshapes", fe.getShapesDerivatives(t))
out("inradius", fe.getElementInradius(aka.Element(t, 7, aka._not_ghost)))
coords = aka.ElementTypeMapArrayReal()
coords.initialize(mesh, nb_component=2)
fe.computeIntegrationPointsCoordinates(coords)
out("xq", coords(t))
