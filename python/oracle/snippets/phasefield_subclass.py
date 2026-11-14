# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (PY-092, D-078) oracle snippet:
# a Python subclass of `PhaseField` (python/py_phasefield.cc: the
# `PyPhaseField` trampoline, `PhaseFieldFactory.registerAllocator`) used
# by a phase-field model: an AT2 law without energy split written in NumPy
# (driving force, driving energy, damage energy and its density from the
# strain the coupler writes, history of `phi`), on the staggered flow of
# phase_field.py (four elements, Eigen pinned, three staggered iterations
# per step). The port runs with its upstream-compatibility switches on
# (port extension setUpstreamCompat, absent upstream).
import numpy as np

import akantu as aka
from _out import out

calls = {}


def count(name):
    calls[name] = calls.get(name, 0) + 1


class PyAT2(aka.PhaseField):
    def __init__(self, model, _id):
        super().__init__(model, _id)
        self.registerParamReal(1.0, "scale", aka._pat_parsmod, "scale")
        self.registerInternalReal("psi", 1)

    def computeDrivingForce(self, el_type, ghost_type):
        count(f"driving.{int(ghost_type)}")
        E, nu = self.getReal("E"), self.getReal("nu")
        l0, gc = self.getReal("l0"), self.getReal("gc")
        lam = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
        mu = E / (2.0 * (1.0 + nu))
        strain = self.getStrain(el_type, ghost_type)
        n = strain.shape[0]
        if n == 0:
            return
        dim = int(round(np.sqrt(strain.shape[1])))
        eps = strain.reshape((n, dim, dim))
        tr = np.einsum("aii->a", eps)
        psi = 0.5 * lam * tr * tr + mu * np.einsum("aij,aij->a", eps, eps)
        self.getInternalReal("psi")(el_type, ghost_type)[:, 0] = psi
        phi = self.getInternalReal("phi")(el_type, ghost_type)[:, 0]
        phi[:] = np.maximum(phi, psi * self.getReal("scale"))
        d = self.getDamage(el_type, ghost_type)[:, 0]
        gradd = self.getInternalReal("grad_d")(el_type, ghost_type)
        self.getInternalReal("driving_force")(el_type, ghost_type)[:, 0] = (
            (2.0 * phi + gc / l0) * d - 2.0 * phi)
        self.getInternalReal("driving_energy")(el_type, ghost_type)[:, :] = (
            gc * l0 * gradd)
        de = self.getInternalReal("damage_energy")(el_type, ghost_type)
        de.reshape((n, dim, dim))[:, :, :] = gc * l0 * np.eye(dim)
        self.getInternalReal("damage_energy_density")(el_type, ghost_type)[
            :, 0] = 2.0 * phi + gc / l0

    def computeDissipatedEnergy(self, el_type):
        count("dissipated")
        l0, gc = self.getReal("l0"), self.getReal("gc")
        d = self.getDamage(el_type)[:, 0]
        gradd = self.getInternalReal("grad_d")(el_type)
        self.getInternalReal("dissipated_energy")(el_type)[:, 0] = gc * (
            d * d / (2.0 * l0) + 0.5 * l0 * np.einsum("ai,ai->a", gradd, gradd))


aka.PhaseFieldFactory.getInstance().registerAllocator(
    "py_at2", lambda dim, split, model, _id: PyAT2(model, _id))

INPUT = """
model solid_mechanics_model [
   material phasefield no_split [
     name = plate
     rho = 1.
     E = 210.0
     nu = 0.3
     Plane_Stress = false
   ]
]

model phase_field_model [
   phasefield py_at2 no_split [
     name = plate
     E = 210.0
     nu = 0.3
     gc = 5e-3
     l0 = 0.3
     Plane_Stress = false
     scale = 1.0
   ]
]
"""
with open("material_py_at2.dat", "w") as f:
    f.write(INPUT)

aka.parseInput("material_py_at2.dat")
mesh = aka.Mesh(2)
mesh.read("test_four_elements.msh")
coupler = aka.CouplerSolidPhaseField(mesh)
if hasattr(coupler, "setUpstreamCompat"):  # the port only
    coupler.setUpstreamCompat(True)
solid = coupler.getSolidMechanicsModel()
phase = coupler.getPhaseFieldModel()
eigen = aka.ModelOptions(
    aka._static, aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen)
)
solid.initFull(eigen)
phase.initFull(eigen)
for model, threshold in ((solid, 1e-8), (phase, 1e-4)):
    solver = model.getNonLinearSolver("static")
    solver.set("max_iterations", 100)
    solver.set("threshold", threshold)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)

pf = phase.getPhaseField(0)
out("pf.class", [type(pf).__name__, type(pf).__mro__[1].__name__])
out("pf.params", [pf.getReal("E"), pf.getReal("gc"), pf.getReal("l0"),
                  pf.getReal("scale")])

X = mesh.getNodes()
bottom = X[:, 1] < X[:, 1].min() + 1e-12
top = X[:, 1] > X[:, 1].max() - 1e-12
left = X[:, 0] < X[:, 0].min() + 1e-12
blocked = solid.getBlockedDOFs()
blocked[bottom, 1] = True
blocked[top, 1] = True
blocked[left, 0] = True
u = solid.getDisplacement()
damage = phase.getDamage()
for step in range(8):
    phase.savePreviousState()
    u[top, 1] += 2e-3
    for _ in range(3):
        coupler.solve("static", "static")
    out(f"pf.{step:02d}.u", u)
    out(f"pf.{step:02d}.damage", damage)
out("pf.epot", solid.getEnergy("potential"))
out("pf.dissipated", phase.getEnergy("dissipated"))
for t in pf.getInternalReal("phi").elementTypes():
    out("pf.phi", pf.getInternalReal("phi")(t))
    out("pf.psi", pf.getInternalReal("psi")(t))
    out("pf.driving_force", pf.getInternalReal("driving_force")(t))
out("pf.calls", sorted((k, v > 0) for k, v in calls.items()))
try:
    aka.PhaseFieldFactory.getInstance().registerAllocator(
        "py_at2", lambda *a: None)
    out("pf.duplicate", "accepted")
except Exception as e:  # noqa: BLE001
    out("pf.duplicate", type(e).__name__)
