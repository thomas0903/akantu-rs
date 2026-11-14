# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 2) oracle snippet: the constitutive-laws handler
# (python/py_constitutive_laws_handler.hh), internal fields
# (python/py_constitutive_law.cc), selectors
# (python/py_constitutive_law_selector.cc) on built-in elastic laws.

import akantu as aka
from _out import out

with open("two.dat", "w") as fh:
    fh.write(
        "material elastic [\n name = soft\n rho = 1.\n nu = 0.3\n E = 1.\n]\n"
        "material elastic [\n name = bulk\n rho = 1.\n nu = 0.3\n E = 10.\n]\n"
    )
aka.parseInput("two.dat")


# a Python selector alternating by call (upstream's Element exposes no
# attribute: the call order is part of the comparison)
class ByParity(aka.ConstitutiveLawSelector):
    def __init__(self):
        super().__init__()
        self.n = 0

    def __call__(self, element):
        self.n += 1
        return self.n % 2


mesh = aka.Mesh(2)
mesh.read("square.msh")
model = aka.SolidMechanicsModel(mesh)
# kept in a variable: upstream loses the Python override of a temporary
# selector object (UD-833)
parity = ByParity()
model.setMaterialSelector(parity)
model.initFull(_analysis_method=aka._static)
out("parity.byel", model.getMaterialByElement()(aka._triangle_3))

# mesh data selector: square.msh names its surface "bulk"
mesh = aka.Mesh(2)
mesh.read("square.msh")
model = aka.SolidMechanicsModel(mesh)
sel = aka.MeshDataMaterialSelectorString("physical_names", model)
model.setMaterialSelector(sel)
model.initFull(_analysis_method=aka._static)
out("meshdata.byel", model.getMaterialByElement()(aka._triangle_3))

# fallback chain: a delegating Python selector with fallback 1
class Delegate(aka.ConstitutiveLawSelector):
    def __call__(self, element):
        return super().__call__(element)


mesh = aka.Mesh(2)
mesh.read("square.msh")
model = aka.SolidMechanicsModel(mesh)
d = Delegate()
d.setFallback(1)
model.setMaterialSelector(d)
model.initFull(_analysis_method=aka._static)
out("fallback.byel", model.getMaterialByElement()(aka._triangle_3))

# handler methods on the default selection
mesh = aka.Mesh(2)
mesh.read("square.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._static)
out("nb_laws", model.getNbConstitutiveLaws())
out("law_index", model.getConstitutiveLawIndex("bulk"))
out("law1.E", model.getConstitutiveLaw(1).getReal("E"))
out("law_by_name.E", model.getConstitutiveLaw("soft").getReal("E"))
el = aka.Element(aka._triangle_3, 3, aka._not_ghost)
out("law_by_el.E", model.getConstitutiveLaw(el).getReal("E"))
out("byel", model.getConstitutiveLawByElement()(aka._triangle_3))
out("numbering", model.getConstitutiveLawLocalNumbering()(aka._triangle_3))
out("isInternal", [model.isInternal("stress"), model.isInternal("bogus")])
try:
    # upstream raises TypeError (unbound return type, UD-832)
    out("laws.n", len(model.getConstitutiveLaws()))
except TypeError:
    pass
model.getDisplacement()[:, 1] = 1e-2 * mesh.getNodes()[:, 1] ** 2
model.assembleInternalForces()
flat = model.flattenInternalReal("stress")
out("flat.stress", flat(aka._triangle_3))
f = flat(aka._triangle_3)
f[:] = 3.0 * f
try:
    # upstream: RuntimeError "Unable to cast from non-held to held instance"
    # (the flattened map is returned by reference, UD-834)
    model.inflateInternalReal("stress", flat)
    out("inflated.stress", model.getMaterial(0).getStress(aka._triangle_3))
except RuntimeError:
    pass
law = model.registerNewConstitutiveLaw("third", "elastic")
out("third.class", type(law).__name__)
out("registerNewMaterial", model.registerNewMaterial("fourth", "elastic"))
out("nb_laws.after", model.getNbConstitutiveLaws())
mat = model.getMaterial(0)
out("mat.class", type(mat).__name__)
stress = mat.getInternalReal("stress")
out("stress.id", stress.getID())
out("stress.types", [str(t) for t in stress.elementTypes()])
out("stress.hasHistory", stress.hasHistory())
mat.setReal("E", 5.0)
mat.setReal(name="rho", value=2.0)
# (setInt on a real parameter: upstream raises "Cannot cast std::any to
# parameter", the port converts, as documented on PY-040)
out("set", [mat.getReal("E"), mat.getReal("rho")])
