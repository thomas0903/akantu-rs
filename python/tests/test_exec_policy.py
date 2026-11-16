# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (no upstream counterpart): the execution-policy setters of
# D-123 (`setNumThreads`, `setPrecision`, `setSimdLevel` and their getters),
# which set the process default of the models created afterwards.
"""Execution policy from Python: round trips, refused values, and an explicit
step whose state is bitwise the same with one and with two threads."""

import os
import subprocess
import sys

import akantu as aka
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


STATE_SCRIPT = r"""
import hashlib, sys
import akantu as aka
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
h = hashlib.sha256()
for a in (model.getDisplacement(), model.getVelocity(), model.getAcceleration(),
          model.getInternalForce()):
    h.update(np.ascontiguousarray(a).tobytes())
print(aka.getNumThreads(), float(np.abs(model.getDisplacement()).max()), h.hexdigest())
"""


def _explicit_state(threads):
    """The state digest of five explicit steps in a fresh interpreter with
    `threads` threads and every batch split as finely as possible
    (`AKANTU_EXEC_GRAIN=finest`: on this small mesh the automatic grain
    would not split any batch)."""
    with open("material_exec.dat", "w") as f:
        f.write(MATERIAL)
    env = dict(os.environ, AKANTU_NUM_THREADS=str(threads), AKANTU_EXEC_GRAIN="finest",
               MPLBACKEND="Agg")
    out = subprocess.run([sys.executable, "-c", STATE_SCRIPT], env=env, check=True,
                         capture_output=True, text=True).stdout.split()
    assert int(out[0]) == threads
    return float(out[1]), out[2]


def test_two_threads_give_the_one_thread_bits(bar_inputs):
    u_max, one = _explicit_state(1)
    assert u_max > 0.0
    for threads in (2, 3):
        assert _explicit_state(threads)[1] == one


def test_setters_replace_an_invalid_environment():
    """With an invalid `AKANTU_NUM_THREADS` the setters still set the
    default (from the serial policy) instead of raising the environment's
    error; the getters raise it until a setter ran."""
    script = (
        "import akantu as aka\n"
        "try:\n    aka.getNumThreads()\n    print('no error')\n"
        "except Exception as e:\n    print('error', 'AKANTU_NUM_THREADS' in str(e))\n"
        "aka.setNumThreads(2)\nprint(aka.getNumThreads())\n"
    )
    env = dict(os.environ, AKANTU_NUM_THREADS="two")
    out = subprocess.run([sys.executable, "-c", script], env=env, check=True,
                         capture_output=True, text=True).stdout.splitlines()
    assert out == ["error True", "2"]
