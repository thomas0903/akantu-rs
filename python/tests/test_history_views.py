# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs regression tests (decision record D-047, deferred history saves
# of internal fields): NumPy views of internal fields held across explicit
# steps see exactly the values of a run that holds no view.
"""Views of internal fields across steps.

The port does not copy a law's current values into its history at every
step while no NumPy view of them exists: the copy is deferred and, when the
law rewrites the whole block, replaced by an exchange of the two
allocations. A block with a live view is copied as upstream does and never
exchanged. These tests run the same explicit problem twice, once holding
views of the current values (and of copies of the previous values) for the
whole run and once taking every value as a copy of a view released at once,
and require the same bits at every step, the views staying attached to the
live arrays, and the previous values of a step being the current values of
the step before."""

import pathlib

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES

TRI = aka._triangle_3
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
    # a stretch large enough to yield the plastic law in the first steps
    x = mesh.getNodes()
    u = model.getDisplacement()
    u[:, 0] = 2e-3 * x[:, 0] * x[:, 1]
    u[:, 1] = -1e-3 * x[:, 0]
    del u
    return model


def run(dat, fields, hold_views):
    """Current and previous values of every field after every step."""
    model = explicit_model(dat)
    mat = model.getMaterial(0)
    internals = {f: mat.getInternalReal(f) for f in fields}
    held = {}
    if hold_views:
        held = {f: internals[f](TRI) for f in fields}
        addresses = {f: address(v) for f, v in held.items()}
    current = {f: [] for f in fields}
    previous = {f: [] for f in fields}
    for _ in range(STEPS):
        model.solveStep()
        for f in fields:
            if hold_views:
                view = held[f]
                # the held view still aliases the live array
                fresh = internals[f](TRI)
                assert address(fresh) == addresses[f]
                assert np.array_equal(bits(view), bits(fresh))
                del fresh
                current[f].append(np.array(view))
            else:
                view = internals[f](TRI)
                current[f].append(np.array(view))
                del view
            previous[f].append(np.array(internals[f].previous(TRI)))
    return current, previous


@pytest.mark.parametrize(
    "dat, fields",
    [(ELASTIC, ELASTIC_FIELDS), (PLASTIC, PLASTIC_FIELDS)],
    ids=["elastic", "plastic"],
)
def test_held_views_see_the_values_of_a_run_without_views(dat, fields):
    cur_free, prev_free = run(dat, fields, hold_views=False)
    cur_held, prev_held = run(dat, fields, hold_views=True)
    for f in fields:
        for k in range(STEPS):
            assert np.array_equal(bits(cur_free[f][k]), bits(cur_held[f][k])), (f, k)
            assert np.array_equal(bits(prev_free[f][k]), bits(prev_held[f][k])), (f, k)
        # the previous values of a step are the current values of the step
        # before (the history is saved at the start of every step)
        for k in range(1, STEPS):
            assert np.array_equal(bits(prev_free[f][k]), bits(cur_free[f][k - 1])), (f, k)
    # the problem moves (and yields): the comparison is not between
    # constant arrays
    assert not np.array_equal(cur_free["stress"][1], cur_free["stress"][-1])
    if "inelastic_strain" in fields:
        assert np.any(cur_free["inelastic_strain"][-1] != 0)


@pytest.mark.parametrize("dat", [ELASTIC, PLASTIC], ids=["elastic", "plastic"])
def test_a_view_taken_between_steps_keeps_aliasing_the_live_values(dat):
    model = explicit_model(dat)
    mat = model.getMaterial(0)
    model.solveStep()
    stress = mat.getStress(TRI)  # exported after a step without views
    grad_u = mat.getGradU(TRI)
    before = np.array(stress)
    prev_copy = np.array(mat.getInternalReal("stress").previous(TRI))
    model.solveStep()
    # the view follows the live array: the new values, at the same address
    assert address(stress) == address(mat.getStress(TRI))
    assert address(grad_u) == address(mat.getGradU(TRI))
    assert np.array_equal(bits(stress), bits(mat.getStress(TRI)))
    assert np.array_equal(
        bits(mat.getInternalReal("stress").previous(TRI)), bits(before)
    )
    # a copy of previous values taken earlier keeps the values of its time
    assert not np.array_equal(prev_copy, mat.getInternalReal("stress").previous(TRI))
    # dropping the views lets the next saves be deferred again, with the
    # same values
    del stress, grad_u
    mid = np.array(mat.getStress(TRI))
    model.solveStep()
    assert np.array_equal(
        bits(mat.getInternalReal("stress").previous(TRI)), bits(mid)
    )
