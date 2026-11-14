# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (PY-077, D-078) oracle snippet:
# Python subclasses of `MaterialLinearElastic2D` (upstream
# `register_material_classes<MaterialElastic<2>>` with the `PyMaterial`
# trampoline, python/py_material.cc): the methods a subclass does not define
# are the elastic law's; `super().initMaterial()`,
# `super().getPushWaveSpeed(...)` and `super().hasStiffnessMatrixChanged()`
# inside an override run the elastic implementation. Flows of upstream
# examples/python/solid_mechanics_model/custom-material/custom-material.py
# (explicit wave in a bar, 30 steps) and a static pull of the free end
# (Eigen pinned through ModelOptions).
import numpy as np

import akantu as aka
from _out import out

calls = {}


def count(name):
    calls[name] = calls.get(name, 0) + 1


class Plain(aka.MaterialLinearElastic2D):
    """No override: the elastic law."""

    def __init__(self, model, _id):
        super().__init__(model, _id)


class InitAndWave(aka.MaterialLinearElastic2D):
    """initMaterial and getPushWaveSpeed call the base; a runtime parameter
    and a runtime internal registered in __init__."""

    def __init__(self, model, _id):
        super().__init__(model, _id)
        self.registerParamReal(0.5, "factor", aka._pat_parsmod, "wave factor")
        self.registerInternalReal("my_field", 2)

    def initMaterial(self):
        count("init")
        super().initMaterial()
        self.lam = self.getReal("lambda")
        self.mu = self.getReal("mu")
        for t in self.getElementFilter().elementTypes():
            self.getInternalReal("my_field")(t)[:] = 3.0

    def getPushWaveSpeed(self, element):
        count("push")
        return super().getPushWaveSpeed(element) * self.getReal("factor")

    def hasStiffnessMatrixChanged(self):
        count("changed")
        return super().hasStiffnessMatrixChanged()


class PythonStress(aka.MaterialLinearElastic2D):
    """computeStress in Python (Hooke's law with the base's Lamé constants);
    computeTangentModuli and the energies are the elastic law's."""

    def initMaterial(self):
        super().initMaterial()
        self.lam = self.getReal("lambda")
        self.mu = self.getReal("mu")

    def computeStress(self, el_type, ghost_type):
        count("stress")
        grad_u = self.getGradU(el_type, ghost_type)
        sigma = self.getStress(el_type, ghost_type)
        n = grad_u.shape[0]
        g = grad_u.reshape((n, 2, 2))
        eps = 0.5 * (g + np.einsum("aij->aji", g))
        trace = np.einsum("aii->a", g)
        sigma.reshape((n, 2, 2))[:, :, :] = (
            np.einsum("a,ij->aij", trace, self.lam * np.eye(2))
            + 2.0 * self.mu * eps
        )


factory = aka.MaterialFactory.getInstance()
for name, cls in (("es_plain", Plain), ("es_init_wave", InitAndWave),
                  ("es_py_stress", PythonStress)):
    factory.registerAllocator(
        name, lambda dim, opt, model, _id, cls=cls: cls(model, _id))

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


def explicit(name, steps=30):
    with open(f"{name}.dat", "w") as f:
        f.write(INPUT.format(name=name))
    aka.parseInput(f"{name}.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar_cm.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    u = model.getDisplacement()
    x = mesh.getNodes()[:, 0] - 5.0
    k = 0.1 * 2 * np.pi * 3
    u[:, 0] = 0.01 * np.sin(k * x) * np.exp(-(k * x) * (k * x))
    mat = model.getMaterial(0)
    out(f"{name}.class", [type(mat).__name__, type(mat).__mro__[1].__name__])
    out(f"{name}.E", mat.getReal("E"))
    out(f"{name}.lambda", mat.getReal("lambda"))
    dt = model.getStableTimeStep()
    out(f"{name}.stable", dt)
    model.setTimeStep(dt * 0.64)
    for step in range(steps):
        model.solveStep()
        if step % 10 == 9:
            out(f"{name}.step{step}.epot", model.getEnergy("potential"))
            out(f"{name}.step{step}.ekin", model.getEnergy("kinetic"))
    out(f"{name}.u", model.getDisplacement())
    for t in mat.getInternalReal("stress").elementTypes():
        out(f"{name}.stress.{int(t)}", mat.getStress(t))
    return model, mat


def static(name):
    with open(f"{name}.dat", "w") as f:
        f.write(INPUT.format(name=name))
    aka.parseInput(f"{name}.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar_cm.msh")
    model = aka.SolidMechanicsModel(mesh)
    # the Eigen pin through the options (the oracle's `_auto` is MUMPS)
    model.initFull(aka.ModelOptions(aka._static, aka.ModelSolverOptions(
        sparse_solver_type=aka.SparseSolverType.eigen)))
    model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    # XBlocked is the right end, YBlocked the top and bottom lines: pull the
    # free left end
    f = model.getExternalForce()
    x = mesh.getNodes()[:, 0]
    f[x < x.min() + 1e-6, 0] = -1e-3
    model.solveStep()
    model.solveStep()  # the second step re-uses K unless the law says so
    out(f"{name}.static.u", model.getDisplacement())
    out(f"{name}.static.epot", model.getEnergy("potential"))
    return model


calls.clear()
explicit("es_plain")
out("es_plain.calls", sorted(calls.items()))

calls.clear()
model, mat = explicit("es_init_wave")
out("es_init_wave.factor", mat.getReal("factor"))
for t in mat.getInternalReal("my_field").elementTypes():
    out(f"es_init_wave.my_field.{int(t)}", mat.getInternalReal("my_field")(t))
out("es_init_wave.calls", sorted((k, v > 0) for k, v in calls.items()))

calls.clear()
explicit("es_py_stress")
out("es_py_stress.calls", sorted((k, v > 0) for k, v in calls.items()))

calls.clear()
static("es_plain")
static("es_init_wave")
out("static.calls", sorted((k, v > 0) for k, v in calls.items()))
static("es_py_stress")
