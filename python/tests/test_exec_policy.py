# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (no upstream counterpart): the execution-policy setters of
# D-123 (`setNumThreads`, `setPrecision`, `setSimdLevel` and their getters),
# which set the process default of the models created afterwards.
"""Execution policy from Python: round trips, refused values, and an explicit
step whose state is bitwise the same with one and with two threads."""

import akantu as aka
import numpy as np
import pytest
from _paths import MESHES

MATERIAL = """material elastic [
  name = body
  rho = 2500
  E = 70e9
  nu = 0.22
]
"""


@pytest.fixture
def restore_policy():
    """Put the process default back (the tests share the interpreter)."""
    saved = (aka.getNumThreads(), aka.getPrecision(), aka.getSimdLevel())
    yield
    aka.setNumThreads(saved[0])
    aka.setPrecision(saved[1])
    aka.setSimdLevel(saved[2])


def test_round_trips(restore_policy):
    aka.setNumThreads(3)
    assert aka.getNumThreads() == 3
    aka.setPrecision("fast")
    assert aka.getPrecision() == "fast"
    aka.setPrecision("bitwise")
    assert aka.getPrecision() == "bitwise"
    aka.setSimdLevel("scalar")
    assert aka.getSimdLevel() == "scalar"
    aka.setSimdLevel("auto")
    assert aka.getSimdLevel() in ("scalar", "avx2", "avx512", "neon")


def test_refused_values(restore_policy):
    for bad in (0, 100000):
        with pytest.raises(Exception):
            aka.setNumThreads(bad)
    with pytest.raises(Exception):
        aka.setPrecision("fastest")
    with pytest.raises(Exception):
        aka.setSimdLevel("sse9")
    assert aka.getNumThreads() >= 1


SC = "examples/python/solid_mechanics_model/solver_callback"


@pytest.fixture
def bar_inputs(upstream_dir):
    upstream_dir(**{"bar.msh": MESHES / SC / "bar.msh"})


def _explicit_state(threads):
    aka.setNumThreads(threads)
    with open("material_exec.dat", "w") as f:
        f.write(MATERIAL)
    aka.parseInput("material_exec.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    model.setTimeStep(model.getStableTimeStep() * 0.8)
    x = mesh.getNodes()
    u = model.getDisplacement()
    u[:, 0] = 1e-4 * np.sin(0.9 * x[:, 0]) * (x[:, 1] + 0.3)
    u[:, 1] = 1e-4 * np.cos(0.5 * x[:, 1]) * x[:, 0]
    for _ in range(5):
        model.solveStep()
    return [np.array(a, copy=True) for a in (model.getDisplacement(), model.getVelocity(),
                                             model.getAcceleration(), model.getInternalForce())]


def test_two_threads_give_the_one_thread_bits(bar_inputs, restore_policy):
    one = _explicit_state(1)
    two = _explicit_state(2)
    assert np.abs(one[0]).max() > 0.0
    for a, b in zip(one, two):
        assert a.tobytes() == b.tobytes()
