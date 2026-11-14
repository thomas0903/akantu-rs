# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet: the behaviours of the probes
# M11-04 (PY-040 `get` of a readable-only parameter), M11-05 (PY-055 the
# FEEngine overloads of `ElementTypeMapArray.initialize`), M11-07 (PY-045 a
# retained `SolverVector.getValues()` after `assembleResidual`), M11-08
# (PY-086 the stand-alone `ContactDetector(mesh[, id])`; the `(mesh,
# positions)` form aborts upstream and is not run), PY-091 (the
# phase-field internal force view) and PY-083 (a `HeatDiffusion2D`
# subclass: constructible, never part of the model). Exception messages
# differ between the bindings; their type and a key phrase are compared.
import numpy as np

import akantu as aka
from _out import out


def raises(key, f, phrase=None):
    try:
        f()
        out(key, "no error")
    except Exception as e:  # noqa: BLE001
        out(key, [type(e).__name__, phrase is not None and phrase in str(e)])


# --- PY-040 ---------------------------------------------------------------------
aka.parseInput("elastic.dat")
mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._static)
mat = model.getMaterial(0)
out("py040.get_E", mat.get("E"))
out("py040.getReal_lambda", mat.getReal("lambda"))
raises("py040.get_lambda", lambda: mat.get("lambda"), "not accessible")
p = aka.Parsable(aka.ParserType.material, "py040")
p.registerParamReal(1.5, "ro", aka._pat_readable, "readable only")
raises("py040.get_ro", lambda: p.get("ro"), "not accessible")
out("py040.getReal_ro", p.getReal("ro"))

# --- PY-055 ---------------------------------------------------------------------
fe = model.getFEEngine()
for i, form in enumerate([
    lambda e: e.initialize(fe, nb_component=2),
    lambda e: e.initialize(fe_engine=fe, nb_component=2),
    lambda e: e.initialize(fe, aka._casper, 2),
    lambda e: e.initialize(mesh=fe, nb_component=2),
]):
    e = aka.ElementTypeMapArrayReal()
    form(e)
    out(f"py055.form{i}", list(e(aka._quadrangle_8).shape))
raises("py055.mesh_positional",
       lambda: aka.ElementTypeMapArrayReal().initialize(mesh, aka._casper, 2))

# --- PY-045 ---------------------------------------------------------------------
dof = model.getDOFManager()
res = dof.getResidual()
v = res.getValues()
out("py045.writeable", bool(v.flags.writeable))
model.applyBC(aka.FixedValue(0.0, aka._x), "edge")
model.applyBC(aka.FixedValue(0.0, aka._y), "edge")
model.getDisplacement()[5, 0] = 1e-3
model.assembleResidual()
out("py045.retained_equals_fresh", bool(np.array_equal(v, res.getValues())))
out("py045.values", v)
v[:] = 0.25
o = np.zeros((mesh.getNbNodes(), 2))
dof.getArrayPerDOFs("displacement", res, o)
out("py045.write_lands", bool(np.all(o == 0.25)))

# --- PY-091 ---------------------------------------------------------------------
aka.parseInput("material_coupling.dat")
m4 = aka.Mesh(2)
m4.read("test_four_elements.msh")
coupler = aka.CouplerSolidPhaseField(m4)
phase = coupler.getPhaseFieldModel()
phase.initFull(_analysis_method=aka._static)
f = phase.getInternalForce()
out("py091.writeable", bool(f.flags.writeable))
phase.assembleInternalForces()
out("py091.follows", bool(np.array_equal(f, phase.getInternalForce())))
f[:] = 7.0
out("py091.write_lands", bool(np.all(phase.getInternalForce() == 7.0)))

# --- PY-086 ---------------------------------------------------------------------
aka.parseInput("detection-explicit.dat")
md = aka.Mesh(2)
md.read("detection-explicit.msh")
d1 = aka.ContactDetector(md)
d2 = aka.ContactDetector(md, "other")
out("py086.classes", [type(d1).__name__, type(d2).__name__])
d1.setSurfaceSelector(aka.PhysicalSurfaceSelector(md))
g = md.getElementGroup("contact_surface")
els = g.getElements()
out("py086.contact_surface", [len(els(t)) for t in els.elementTypes(dim=1)])
aka.parseInput("elastic.dat")
raises("py086.no_section", lambda: aka.ContactDetector(md))

# --- PY-083 ---------------------------------------------------------------------
calls = []


class MyHeat(aka.HeatDiffusion2D):
    def __init__(self, model, _id):
        super().__init__(model, _id)
        self.registerInternalReal("my_field", 1)

    def computeDiffusivityOnQuadPoints(self, el_type, ghost_type):
        calls.append("diffusivity")


aka.parseInput("heat_transfer_input.dat")
mh = aka.Mesh(2)
mh.read("mesh_dcb_2d.msh")
heat = aka.HeatTransferModel(mh)
heat.initFull(aka._static)
n = heat.getNbConstitutiveLaws()
h = MyHeat(heat, "mine")
out("py083.class", [type(h).__name__, type(h).__mro__[1].__name__])
out("py083.params", [h.getReal("density"), h.getReal("capacity"),
                     h.getReal("conductivity_variation")])
# (a `setReal` before the conductivity is set crashes upstream:
# `updateInternalParameters` reads the empty conductivity, UD-285; the port
# refuses it, pinned by test_py3_laws.py)
h.set("conductivity", np.eye(2))
h.setReal("density", 3.0)
out("py083.density", h.getReal("density"))
out("py083.laws_unchanged", heat.getNbConstitutiveLaws() == n)
out("py083.calls", calls)
raises("py083.bare", lambda: type("Bare", (aka.DiffusionLaw,), {})(heat, "y"))
