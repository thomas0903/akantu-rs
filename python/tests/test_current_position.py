# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests: getCurrentPosition()
# inside callbacks of a running call returns X + u, as upstream's
# SolidMechanicsModel::getCurrentPosition() (solid_mechanics_model.cc:411-429,
# python/py_solid_mechanics_model.cc) updates before returning the array. The
# three callback kinds are those of program A7, whose oracle run gives
# max |cp - (X + u)| = 0.0 in all 17 calls (the oracle output is not
# committed); the port gave up to 1.1e-3 before the fix.
"""getCurrentPosition() inside solver callbacks, Python materials and
Dirichlet functors."""

import itertools
import pathlib

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES

SC = "examples/python/solid_mechanics_model/solver_callback"
CM = "examples/python/solid_mechanics_model/custom-material"


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "bar.msh": MESHES / SC / "bar.msh",
            "material.dat": PARSER_FIXTURES / SC / "material.dat",
            "bar_cm.msh": MESHES / CM / "bar.msh",
            "material_cm.dat": PARSER_FIXTURES / CM / "material.dat",
        }
    )


_names = itertools.count()


def parse_custom_material(cls):
    """Register ``cls`` under a fresh factory id (the factory is
    process-global and refuses an id twice) and parse upstream's
    custom-material input with that id."""
    name = f"local_elastic_cp_{next(_names)}"
    aka.MaterialFactory.getInstance().registerAllocator(
        name, lambda _d, _u, model, _id: cls(model, _id))
    text = pathlib.Path("material_cm.dat").read_text()
    pathlib.Path(f"{name}.dat").write_text(text.replace("local_elastic", name))
    aka.parseInput(f"{name}.dat")


def eig(method):
    return aka.ModelOptions(
        method,
        aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen))


def gap(model, mesh):
    """max |current_position - (X + u)|, exactly 0.0 when up to date (the
    port computes X + u with the same sums as upstream)."""
    cp = model.getCurrentPosition()
    return float(np.abs(cp - (mesh.getNodes() + model.getDisplacement())).max())


def bar_model():
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(eig(aka._implicit_dynamic))
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    X = mesh.getNodes()
    model.getVelocity()[:, 0] = 1e-2 * np.sin(0.6 * (X[:, 0] - 5.0))
    model.setTimeStep(model.getStableTimeStep() * 0.64)
    return mesh, model


def test_inside_intercept_solver_callback():
    mesh, model = bar_model()
    assert gap(model, mesh) == 0.0  # fetched (and updated) before the solve
    seen = []

    class Probe(aka.InterceptSolverCallback):
        def __init__(self, model):
            super().__init__(model)
            self.model = model

        def assembleResidual(self):
            seen.append(gap(self.model, mesh))
            self.model.assembleResidual()

        def afterSolveStep(self, converged):
            self.model.afterSolveStep(converged)
            seen.append(gap(self.model, mesh))

    for _ in range(3):
        model.solveStep(Probe(model))
    assert len(seen) == 12 and seen == [0.0] * 12, seen
    assert gap(model, mesh) == 0.0
    # the position really moved (the probe is not vacuous)
    assert np.abs(model.getDisplacement()).max() > 1e-5


def test_inside_dirichlet_functor_after_a_solve():
    mesh, model = bar_model()
    seen = []

    class Probing(aka.DirichletFunctor):
        def __init__(self):
            super().__init__(aka._x)
            self.done = False

        def __call__(self, node, flags, disp, coord):
            if not self.done:
                seen.append(gap(model, mesh))
                self.done = True

    model.solveStep()
    model.applyBC(Probing(), "YBlocked")
    assert seen == [0.0]


class LocalElastic(aka.Material):
    seen = []

    def __init__(self, model, _id):
        super().__init__(model, _id)
        super().registerParamReal("E", aka._pat_readable | aka._pat_parsable, "E")
        super().registerParamReal("nu", aka._pat_readable | aka._pat_parsable, "nu")

    def initMaterial(self):
        nu, E = self.getReal("nu"), self.getReal("E")
        self.lam = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
        self.mu = E / (2.0 * (1.0 + nu))
        super().initMaterial()

    def getPushWaveSpeed(self, element):
        return np.sqrt((self.lam + 2 * self.mu) / self.getReal("rho"))

    def computeStress(self, el_type, ghost_type):
        m = self.getModel()
        LocalElastic.seen.append(gap(m, m.getMesh()))
        g = self.getGradU(el_type, ghost_type).reshape((-1, 2, 2))
        s = self.getStress(el_type, ghost_type).reshape((-1, 2, 2))
        eps = 0.5 * (g + np.einsum("aij->aji", g))
        tr = np.einsum("aii->a", g)
        s[:, :, :] = (np.einsum("a,ij->aij", tr, self.lam * np.eye(2))
                      + 2 * self.mu * eps)


def test_inside_python_material():
    LocalElastic.seen = []
    parse_custom_material(LocalElastic)
    mesh = aka.Mesh(2)
    mesh.read("bar_cm.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    X = mesh.getNodes()
    model.getVelocity()[:, 0] = 1e-2 * np.sin(0.9 * (X[:, 0] - 5.0))
    model.setTimeStep(model.getStableTimeStep() * 0.64)
    for _ in range(4):
        model.solveStep()
    assert len(LocalElastic.seen) == 4 and LocalElastic.seen == [0.0] * 4
    assert np.abs(model.getDisplacement()).max() > 0.0
