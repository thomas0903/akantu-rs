# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs regression tests (decision record D-047, deferred history saves
# of internal fields): NumPy views of internal fields held across explicit
# steps see exactly the values of a run that exports nothing.
"""Views of internal fields across steps.

The port does not copy a law's current values into its history at every
step while the block is not exported to NumPy: the copy is deferred and,
when the law rewrites the whole block, replaced by an exchange of the two
allocations. A block Python has fetched stays exported (the binding keeps
its lease) and is copied at every save as upstream does, never exchanged.

These tests run the same explicit problem twice: once holding views of the
current values for the whole run (every save is a copy), and once reading
only copies of the previous values and the energies, so that nothing is
exported until the end (every save is deferred). They require the same bits
at every step, the held views staying attached to the live arrays and
seeing every new value, and the previous values of a step being the current
values of the step before."""

import pathlib

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES

QUAD = aka._quadrangle_8  # the element type of the DCB mesh
STEPS = 12
ELASTIC_FIELDS = ("stress", "grad_u", "sigma_th", "epsilon_th")
PLASTIC_FIELDS = ELASTIC_FIELDS + ("iso_hardening", "inelastic_strain")

ELASTIC = """material elastic [
  name = bulk
  rho = 2500
  nu = 0.22
  E = 71e9
]
"""

PLASTIC = """material plastic_linear_isotropic_hardening [
  name = bulk
  rho = 2500
  nu = 0.22
  E = 71e9
  h = 1e9
  sigma_y = 2e7
]
"""

CASES = pytest.mark.parametrize(
    "dat, fields",
    [(ELASTIC, ELASTIC_FIELDS), (PLASTIC, PLASTIC_FIELDS)],
    ids=["elastic", "plastic"],
)


@pytest.fixture(autouse=True)
def _dcb(upstream_dir):
    upstream_dir(
        **{
            "mesh_dcb_2d.msh": MESHES
            / "test/test_python_interface/mesh_dcb_2d.msh",
        }
    )


def bits(a):
    return np.array(a, dtype=np.float64).view(np.uint64)


def address(view):
    return view.__array_interface__["data"][0]


def explicit_model(dat):
    pathlib.Path("material.dat").write_text(dat)
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    model.setTimeStep(0.5 * model.getStableTimeStep())
    # a stretch and a shear (strains of 2e-3 and 5e-3) that yield the
    # plastic law in the first steps
    x = mesh.getNodes()
    u = model.getDisplacement()
    u[:, 0] = 2e-3 * x[:, 0]
    u[:, 1] = 5e-3 * x[:, 0]
    del u
    return model


def run(dat, fields, hold_views):
    """Per step: the previous values of every field (copies), the current
    values seen through the held views (when held) and the potential
    energy; at the end the current values."""
    model = explicit_model(dat)
    mat = model.getMaterial(0)
    internals = {f: mat.getInternalReal(f) for f in fields}
    held = {f: internals[f](QUAD) for f in fields} if hold_views else {}
    addresses = {f: address(v) for f, v in held.items()}
    previous = {f: [] for f in fields}
    current = {f: [] for f in fields}
    energy = []
    for _ in range(STEPS):
        model.solveStep()
        for f in fields:
            # `previous(type)` is a copy: it exports nothing
            previous[f].append(np.array(internals[f].previous(QUAD)))
            if hold_views:
                fresh = internals[f](QUAD)
                assert address(held[f]) == address(fresh) == addresses[f], f
                assert np.array_equal(bits(held[f]), bits(fresh)), f
                del fresh
                current[f].append(np.array(held[f]))
        energy.append(model.getEnergy("potential"))
    final = {f: np.array(internals[f](QUAD)) for f in fields}
    return previous, current, energy, final


@CASES
def test_held_views_see_the_values_of_a_run_that_exports_nothing(dat, fields):
    prev_free, _, energy_free, final_free = run(dat, fields, hold_views=False)
    prev_held, cur_held, energy_held, final_held = run(dat, fields, hold_views=True)
    assert np.array_equal(
        np.array(energy_free).view(np.uint64), np.array(energy_held).view(np.uint64)
    )
    for f in fields:
        for k in range(STEPS):
            assert np.array_equal(bits(prev_free[f][k]), bits(prev_held[f][k])), (f, k)
        assert np.array_equal(bits(final_free[f]), bits(final_held[f])), f
        assert np.array_equal(bits(final_held[f]), bits(cur_held[f][-1])), f
        # the previous values of a step are the current values of the step
        # before (the history is saved at the start of every step)
        for k in range(1, STEPS):
            assert np.array_equal(bits(prev_held[f][k]), bits(cur_held[f][k - 1])), (f, k)
    # the problem moves (and yields): the comparisons are not between
    # constant arrays
    assert not np.array_equal(cur_held["stress"][1], cur_held["stress"][-1])
    if "inelastic_strain" in fields:
        assert np.any(final_free["inelastic_strain"] != 0)


@CASES
def test_a_view_taken_between_steps_keeps_aliasing_the_live_values(dat, fields):
    model = explicit_model(dat)
    mat = model.getMaterial(0)
    for _ in range(3):
        model.solveStep()  # nothing exported: deferred saves
    stress = mat.getStress(QUAD)
    grad_u = mat.getGradU(QUAD)
    before = {"stress": np.array(stress), "grad_u": np.array(grad_u)}
    prev_copy = np.array(mat.getInternalReal("stress").previous(QUAD))
    model.solveStep()
    # the views follow the live arrays: the new values, at the same address
    assert address(stress) == address(mat.getStress(QUAD))
    assert address(grad_u) == address(mat.getGradU(QUAD))
    assert np.array_equal(bits(stress), bits(mat.getStress(QUAD)))
    assert not np.array_equal(bits(stress), bits(before["stress"]))
    for f in ("stress", "grad_u"):
        assert np.array_equal(
            bits(mat.getInternalReal(f).previous(QUAD)), bits(before[f])
        ), f
    # a copy of previous values taken earlier keeps the values of its time
    assert not np.array_equal(prev_copy, mat.getInternalReal("stress").previous(QUAD))
