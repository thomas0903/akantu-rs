# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet, run on both modules in
# the same image and compared with compare.py at tolerance 0. Upstream
# sources: python/py_material.cc (PyMaterial trampoline, PYBIND11_OVERRIDE ->
# pybind11's get_override frame rule; bound hasMatrixChanged /
# hasStiffnessMatrixChanged / initMaterial / getPushWaveSpeed /
# getShearWaveSpeed), src/model/solid_mechanics/material.hh:479-490
# (hasMatrixChanged), src/io/parser/parameter_registry_tmpl.hh:374-386 (a
# second registration raises),
# python/py_solver.cc:85-92 (SolverVector.getValues).
# Inputs (work dir): bar_cm.msh.
#
# Keys: p301.* the trampoline rule inside a running callback (every
# overridable bound method called from another method of the class runs the
# override), p301.top_init.* a top-level initMaterial() of a subclass (its
# super-call is the base's), p302.* second registrations, p306.*
# hasMatrixChanged, p6.* the retained residual view through
# implicit dynamics (M·a and C·v through the matrix-product paths). The generic get of non-Real parameters (UD-1174, a difference by
# design) is in ud1174_get_types.py.
import numpy as np

import akantu as aka
from _out import out

INPUT = """
model solid_mechanics_model [
  material {name} [
    name = bar
    rho = 1.
    E = 1.
    nu = .3
  ]
]
"""


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
    return mesh, model


def pull(mesh, model):
    x = mesh.getNodes()[:, 0]
    model.getExternalForce()[x <= x.min() + 1e-9, 0] = -1e-3


def linear_stress(mat, el_type, ghost_type):
    lam, mu = mat.getReal("lambda"), mat.getReal("mu")
    grad_u = mat.getGradU(el_type, ghost_type)
    sigma = mat.getStress(el_type, ghost_type)
    n = grad_u.shape[0]
    g = grad_u.reshape((n, 2, 2))
    eps = 0.5 * (g + np.einsum("aij->aji", g))
    trace = np.einsum("aii->a", g)
    sigma.reshape((n, 2, 2))[:, :, :] = (
        np.einsum("a,ij->aij", trace, lam * np.eye(2)) + 2.0 * mu * eps)


def outcome(f):
    try:
        r = f()
        return ["ok", r if isinstance(r, (bool, int, float, str)) else None]
    except Exception as e:  # noqa: BLE001
        return [type(e).__name__, str(e).splitlines()[0][:120]]


# ---: wave speeds and hasStiffnessMatrixChanged in a callback -----------
seen = {}


class Waves(aka.MaterialLinearElastic2D):
    def getPushWaveSpeed(self, element):
        return 0.25 * super().getPushWaveSpeed(element)

    def getShearWaveSpeed(self, element):
        return 3.0 * super().getShearWaveSpeed(element)

    def hasStiffnessMatrixChanged(self):
        seen["changed_calls"] = seen.get("changed_calls", 0) + 1
        return False

    def computeStress(self, el_type, ghost_type):
        linear_stress(self, el_type, ghost_type)
        if "p_super" not in seen and ghost_type == aka._not_ghost:
            el = aka.Element(el_type, 0, aka._not_ghost)
            seen["p_super"] = super().getPushWaveSpeed(el)
            seen["p_class"] = aka.Material.getPushWaveSpeed(self, el)
            seen["p_self"] = self.getPushWaveSpeed(el)
            seen["s_super"] = super().getShearWaveSpeed(el)
            seen["s_class"] = aka.Material.getShearWaveSpeed(self, el)
            seen["k_super"] = super().hasStiffnessMatrixChanged()
            seen["k_class"] = aka.Material.hasStiffnessMatrixChanged(self)
            seen["k_matrix"] = super().hasMatrixChanged("K")
            seen["m_matrix"] = super().hasMatrixChanged("M")


register("py3fix_waves", Waves)
mesh, model = bar("py3fix_waves", aka._explicit_lumped_mass)
model.setTimeStep(model.getStableTimeStep() * 0.5)
model.solveStep()
for k in sorted(seen):
    if k != "changed_calls":
        out(f"p301.waves.{k}", seen[k])
mat = model.getMaterial(0)
t = mat.getElementFilter().elementTypes()[0]
el = aka.Element(t, 0, aka._not_ghost)
out("p301.waves.top_push", mat.getPushWaveSpeed(el))
before = seen.get("changed_calls", 0)
out("p301.waves.top_changed", mat.hasStiffnessMatrixChanged())
out("p301.waves.top_changed_class", aka.Material.hasStiffnessMatrixChanged(mat))
out("p301.waves.top_matrix_K", mat.hasMatrixChanged("K"))
out("p301.waves.top_changed_calls", seen.get("changed_calls", 0) - before)
out("p301.waves.top_shear_class", aka.Material.getShearWaveSpeed(mat, el))
out("p301.waves.stable_dt", model.getStableTimeStep())
out("p301.waves.displacement", model.getDisplacement())

# ---: initMaterial from another method, and at top level ----------------
calls = {"init": 0}


class Init(aka.MaterialLinearElastic2D):
    def initMaterial(self):
        calls["init"] += 1
        super().initMaterial()

    def computeStress(self, el_type, ghost_type):
        if "in_stress" not in calls:
            calls["in_stress"] = calls["init"]
            calls["in_stress_outcome"] = outcome(lambda: super(Init, self).initMaterial())
            calls["in_stress_delta"] = calls["init"] - calls["in_stress"]
        linear_stress(self, el_type, ghost_type)


register("py3fix_init", Init)
mesh, model = bar("py3fix_init", aka._static)
out("p301.init.after_init_full", calls["init"])
pull(mesh, model)
model.solveStep()
out("p301.init.in_stress_outcome", calls["in_stress_outcome"])
out("p301.init.in_stress_delta", calls["in_stress_delta"])
before = calls["init"]
model.getMaterial(0).initMaterial()
out("p301.top_init.delta", calls["init"] - before)
out("p301.init.displacement", model.getDisplacement())

# ---: second registrations ----------------------------------------------
for pname in ("E", "nu", "lambda", "rho"):
    class Dup(aka.MaterialLinearElastic2D):
        def __init__(self, model, _id, pname=pname):
            super().__init__(model, _id)
            self.registerParamReal(pname, aka._pat_parsmod, "dup")

    register(f"py3fix_dup_{pname}", Dup)
    r = outcome(lambda pname=pname: bar(f"py3fix_dup_{pname}", aka._static))
    out(f"p302.elastic.{pname}", [r[0], "already registered" in str(r[1])])

# ---: hasMatrixChanged of the bound elastic law -------------------------
mesh_e, model_e = bar("elastic", aka._static)
me = model_e.getMaterial(0)
for mid in ("K", "M", "X"):
    out(f"p306.elastic.{mid}", bool(me.hasMatrixChanged(mid)))
pull(mesh_e, model_e)
model_e.solveStep()
for mid in ("K", "M"):
    out(f"p306.elastic_after_solve.{mid}", bool(me.hasMatrixChanged(mid)))

# --- (M1): the retained residual through implicit dynamics --------------
mesh_d, model_d = bar("elastic", aka._implicit_dynamic)
pull(mesh_d, model_d)
model_d.setTimeStep(0.05)
res = model_d.getDOFManager().getResidual()
v = res.getValues()
for step in range(3):
    model_d.solveStep()
    out(f"p6.step{step}.retained_equals_fresh",
        bool(np.array_equal(v, res.getValues())))
    out(f"p6.step{step}.retained", np.array(v))
out("p6.displacement", model_d.getDisplacement())
