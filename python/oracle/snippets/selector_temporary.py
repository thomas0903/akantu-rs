# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 2) oracle probe of UD-833: a temporary Python
# selector object given to setMaterialSelector (expected to differ).
import akantu as aka
from _out import out
with open("two.dat", "w") as fh:
    fh.write("material elastic [\n name = soft\n rho = 1.\n nu = 0.3\n E = 1.\n]\n"
             "material elastic [\n name = bulk\n rho = 1.\n nu = 0.3\n E = 10.\n]\n")
aka.parseInput("two.dat")
class ByParity(aka.ConstitutiveLawSelector):
    n = 0

    def __call__(self, element):
        ByParity.n += 1
        return ByParity.n % 2
mesh = aka.Mesh(2)
mesh.read("square.msh")
model = aka.SolidMechanicsModel(mesh)
model.setMaterialSelector(ByParity())
model.initFull(_analysis_method=aka._static)
out("temp.byel.sum", int(model.getMaterialByElement()(aka._triangle_3).sum()))
