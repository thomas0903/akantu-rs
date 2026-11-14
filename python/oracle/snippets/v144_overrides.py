# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet:
# the three overridable methods of a MaterialLinearElastic2D
# subclass that the other tests and snippets do not exercise
# (python/py_material.cc PyMaterial: computeTangentModuli,
# computePotentialEnergy, getShearWaveSpeed), on both modules.
# Inputs (work dir): bar_cm.msh.
import numpy as np

import akantu as aka
from _out import out

INPUT = """
model solid_mechanics_model [
  sparse_solver_type = eigen
  material {name} [
    name = bar
    rho = 1.
    E = 1.
    nu = .3
  ]
]
"""
calls = {}


def count(k):
    calls[k] = calls.get(k, 0) + 1


def register(name, cls):
    aka.MaterialFactory.getInstance().registerAllocator(
        name, lambda dim, opt, model, _id: cls(model, _id))


def bar(law, method):
    with open(f"{law}.dat", "w") as f:
        f.write(INPUT.format(name=law))
    aka.parseInput(f"{law}.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar_cm.msh")
    model = aka.SolidMechanicsModel(mesh)
    # the Eigen pin through the options (the oracle's `_auto` is MUMPS)
    model.initFull(aka.ModelOptions(method, aka.ModelSolverOptions(
        sparse_solver_type=aka.SparseSolverType.eigen)))
    model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    x = mesh.getNodes()[:, 0]
    model.getExternalForce()[x <= x.min() + 1e-9, 0] = -1e-3
    return mesh, model


class Tangent(aka.MaterialLinearElastic2D):
    """The plane-strain elastic tangent written from NumPy."""

    def computeTangentModuli(self, el_type, tangent, ghost_type):
        count("tangent")
        lam, mu = self.getReal("lambda"), self.getReal("mu")
        d = np.array([[lam + 2 * mu, lam, 0.0], [lam, lam + 2 * mu, 0.0],
                      [0.0, 0.0, mu]])
        if "tangent_shape" not in calls:
            calls["tangent_shape"] = list(tangent.shape)
        tangent.reshape((tangent.shape[0], 3, 3))[:] = d


class Energy(aka.MaterialLinearElastic2D):
    def computePotentialEnergy(self, el_type):
        count("energy")
        self.getPotentialEnergy(el_type)[:] = 1.5


class Shear(aka.MaterialLinearElastic2D):
    def getShearWaveSpeed(self, element):
        count("shear")
        return 3.0 * super().getShearWaveSpeed(element)


register("v144_tangent", Tangent)
register("v144_energy", Energy)
register("v144_shear", Shear)

mesh, model = bar("v144_tangent", aka._static)
model.solveStep()
out("tangent.u", model.getDisplacement())
out("tangent.called", calls.get("tangent", 0) > 0)
out("tangent.shape", calls.get("tangent_shape"))
mesh_e, ref = bar("elastic", aka._static)
ref.solveStep()
out("tangent.equals_elastic", bool(np.array_equal(model.getDisplacement(),
                                                  ref.getDisplacement())))

mesh, model = bar("v144_energy", aka._static)
model.solveStep()
out("energy.potential", model.getEnergy("potential"))
out("energy.called", calls.get("energy", 0) > 0)

mesh, model = bar("v144_shear", aka._explicit_lumped_mass)
mat = model.getMaterial(0)
t = mat.getElementFilter().elementTypes()[0]
el = aka.Element(t, 0, aka._not_ghost)
out("shear.bound", mat.getShearWaveSpeed(el))
out("shear.class", aka.Material.getShearWaveSpeed(mat, el))
out("shear.elastic", ref.getMaterial(0).getShearWaveSpeed(el))
out("shear.calls", calls.get("shear", 0))
