# SPDX-License-Identifier: LGPL-3.0-or-later
# Copied unchanged from Akantu (EPFL/LSMS): test/test_model/patch_tests/test_patch_linear_elastic_static.py
# (upstream commit 35a85178); run by pytest from a directory holding the
# upstream meshes and material files (conftest.py).
#!/usr/bin/env python3
__copyright__ = (
    "Copyright (©) 2018-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)"
    "Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)"
)
__license__ = "LGPLv3"


import sys
from patch_test_linear_solid_mechanics_fixture import TestPatchTestSMMLinear
import akantu


def foo(self):
    filename = "material_check_stress_plane_stress.dat"
    if self.plane_strain:
        filename = "material_check_stress_plane_strain.dat"

    self.initModel(akantu._static, filename)

    solver = self.model.getNonLinearSolver()
    solver.set("max_iterations", 2)
    solver.set("threshold", 2e-4)
    solver.set("convergence_type", akantu.SolveConvergenceCriteria.residual)

    self.model.solveStep()

    self.checkAll()


def test():
    TestPatchTestSMMLinear.TYPED_TEST(foo, "Static")


if "pytest" not in sys.modules:
    test()
