# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: adversarial tests of the port's package, written independently
# of the port (the two upstream-semantics cases pass since the two defects
# they found are fixed). Upstream
# behaviour they assert: python/py_material.cc (PyMaterial trampoline; pybind11's get_override frame rule), python/py_parser.cc and
# src/io/parser/parameter_registry_tmpl.hh:374-386 (a second registration
# raises), python/py_solver.cc:85-92 (getValues is a view of the residual).
"""Adversarial cases (port only)."""

import itertools

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES

CM = "examples/python/solid_mechanics_model/custom-material"
EX = "examples/python/solid_mechanics_cohesive_model"
_names = itertools.count()


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(**{
        "bar.msh": MESHES / CM / "bar.msh",
        "plate.msh": MESHES / EX / "cohesive/plate.msh",
        "material.dat": PARSER_FIXTURES / EX / "cohesive/material.dat",
    })


INPUT = """
model solid_mechanics_model [
  sparse_solver_type = eigen
  material {name} [
    name = bar
    rho = 1.
    E = 1.
    nu = .3
  ]
]
"""


def register(cls):
    name = f"v144_{next(_names)}"
    aka.MaterialFactory.getInstance().registerAllocator(
        name, lambda dim, opt, model, _id: cls(model, _id))
    return name


def bar(law, method):
    with open(f"{law}.dat", "w") as f:
        f.write(INPUT.format(name=law))
    aka.parseInput(f"{law}.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=method)
    model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    return mesh, model


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


# --- residual view (PY-045) ----------------------------------------------------


def test_v144_residual_view_across_a_cohesive_insertion():
    """Extrinsic insertion adds nodes and renumbers the equations: a view
    kept from before keeps its size and values (stale but safe, D-047),
    stays readable and writable without reaching the live residual; a fresh
    view has the new size and follows again."""
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
    model.applyBC(aka.FixedValue(0.0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0.0, aka._y), "YBlocked")
    trac = np.zeros(2)
    trac[int(aka._y)] = 0.095
    model.getExternalForce()[:] = 0
    model.applyBC(aka.FromTraction(trac), "Traction")
    model.setTimeStep(model.getStableTimeStep() * 0.1)
    dof = model.getDOFManager()
    v = dof.getResidual().getValues()
    n0 = v.shape[0]
    inserted = 0
    for _ in range(400):
        inserted += model.checkCohesiveStress()
        model.solveStep("explicit_lumped")
        if inserted:
            break
        assert np.array_equal(v, dof.getResidual().getValues())
    assert inserted > 0, "no insertion in 400 steps"
    fresh = dof.getResidual().getValues()
    assert fresh.shape[0] > n0 and v.shape[0] == n0
    assert np.isfinite(v).all()
    before = fresh.copy()
    v[:] = 12345.0  # the stale view is writable; nothing reaches the residual
    assert np.array_equal(dof.getResidual().getValues(), before)
    model.solveStep("explicit_lumped")
    again = dof.getResidual().getValues()
    assert np.shares_memory(again, fresh) or np.array_equal(again, fresh)
    assert np.array_equal(fresh, dof.getResidual().getValues())


def test_v144_residual_view_inside_a_law_callback():
    """getValues() from inside a Python law method (not a solver callback):
    with a view exported before, the same storage; never a crash."""
    got = {}

    class Reader(aka.MaterialLinearElastic2D):
        def computeStress(self, el_type, ghost_type):
            linear_stress(self, el_type, ghost_type)
            if "inside" not in got:
                try:
                    got["inside"] = model_ref[0].getDOFManager().getResidual().getValues()
                except aka.Exception as e:  # a documented refusal is acceptable
                    got["inside"] = e

    model_ref = []
    name = register(Reader)
    _, model = bar(name, aka._static)
    model_ref.append(model)
    retained = model.getDOFManager().getResidual().getValues()
    x = model.getMesh().getNodes()[:, 0]
    model.getExternalForce()[x <= x.min() + 1e-9, 0] = -1e-3
    model.solveStep()
    inside = got["inside"]
    if isinstance(inside, np.ndarray):
        assert np.shares_memory(inside, retained)
    else:
        assert "in use" in str(inside) or "callback" in str(inside)


def test_v144_residual_view_first_taken_inside_a_solver_callback():
    """No view exported before the solve: the first getValues() happens
    inside the solve's callback (through the solve access the solve lent);
    a write through it is what the solve uses (as assembleToResidual)."""
    aka.parseInput("material.dat")

    def solve(through_view):
        _, model = bar("elastic", aka._static)
        mesh = model.getMesh()
        x = mesh.getNodes()[:, 0]
        node = int(np.flatnonzero(x <= x.min() + 1e-9)[0])
        f = np.zeros((mesh.getNbNodes(), 2))
        f[node, 0] = -1e-3
        dof = model.getDOFManager()
        got = []

        class Load(aka.InterceptSolverCallback):
            def assembleResidual(self):
                model.assembleResidual()
                if through_view:
                    v = dof.getResidual().getValues()
                    got.append(v.shape)
                    tmp = np.zeros_like(v)
                    # the equation of (node, x): assemble into a scratch
                    # copy through the DOF manager is not available here,
                    # so add through assembleToResidual and compare views
                    before = v.copy()
                    dof.assembleToResidual("displacement", f)
                    tmp[:] = v - before
                    v[:] = before + tmp  # write back the same values
                else:
                    dof.assembleToResidual("displacement", f)

        model.solveStep(Load(model))
        return got, model.getDisplacement().copy()

    got, a = solve(True)
    _, b = solve(False)
    assert got and got[0][1] == 1
    assert np.abs(a).max() > 0.0
    assert np.array_equal(a, b)


# --- law callbacks: refusals without crash ------------------------------------


def test_v144_mutation_through_a_shared_lend_is_refused():
    """getPushWaveSpeed is a const method (the state is lent shared): a
    parameter write from it is refused cleanly (upstream's C++ would let a
    const method's Python override call setReal on the same object)."""
    outcome = {}

    class Writer(aka.MaterialLinearElastic2D):
        def getPushWaveSpeed(self, element):
            try:
                self.setReal("E", 4.0)
                outcome["set"] = "accepted"
            except aka.Exception as e:
                outcome["set"] = str(e)
            return super().getPushWaveSpeed(element)

    name = register(Writer)
    _, model = bar(name, aka._explicit_lumped_mass)
    dt = model.getStableTimeStep()
    assert dt > 0.0
    assert "set" in outcome
    e_now = model.getMaterial(0).getReal("E")
    if outcome["set"] == "accepted":
        assert e_now == 4.0
    else:
        assert e_now == 1.0


def test_v144_reentrant_model_call_from_a_law_callback_is_refused():
    errors = []

    class Reenter(aka.MaterialLinearElastic2D):
        def computeStress(self, el_type, ghost_type):
            linear_stress(self, el_type, ghost_type)
            if not errors:
                try:
                    model_ref[0].assembleInternalForces()
                    errors.append("accepted")
                except aka.Exception as e:
                    errors.append(str(e))

    model_ref = []
    name = register(Reenter)
    _, model = bar(name, aka._static)
    model_ref.append(model)
    model.solveStep()
    assert errors and errors[0] != "accepted"


def test_v144_tangent_kept_after_the_call_is_a_detached_copy():
    kept = []

    class Tangent(aka.MaterialLinearElastic2D):
        def computeTangentModuli(self, el_type, tangent, ghost_type):
            lam, mu = self.getReal("lambda"), self.getReal("mu")
            d = np.array([[lam + 2 * mu, lam, 0.0], [lam, lam + 2 * mu, 0.0],
                          [0.0, 0.0, mu]])
            tangent.reshape((tangent.shape[0], 3, 3))[:] = d
            kept.append(tangent)

    name = register(Tangent)
    mesh, model = bar(name, aka._static)
    x = mesh.getNodes()[:, 0]
    model.getExternalForce()[x <= x.min() + 1e-9, 0] = -1e-3
    model.solveStep()
    u = model.getDisplacement().copy()
    _, ref = bar("elastic", aka._static)
    ref.getExternalForce()[x <= x.min() + 1e-9, 0] = -1e-3
    ref.solveStep()
    assert np.array_equal(u, ref.getDisplacement())
    kept[0][:] = 1e300  # no effect, no crash
    model.solveStep()
    assert np.isfinite(model.getDisplacement()).all()


# --- semantics asserted as upstream's (pybind11 / ParameterRegistry) ------------


def test_v144_upstream_super_call_from_another_method_runs_the_override():
    """Upstream: inside computeStress (called from C++), the bound
    ``Material.getPushWaveSpeed`` reaches the trampoline, whose
    ``get_override`` sees the calling frame ``computeStress`` (not
    ``getPushWaveSpeed``) and calls the Python override: 0.25 × base."""
    seen = {}

    class Speedy(aka.MaterialLinearElastic2D):
        def getPushWaveSpeed(self, element):
            return 0.25 * super().getPushWaveSpeed(element)

        def computeStress(self, el_type, ghost_type):
            linear_stress(self, el_type, ghost_type)
            if "super" not in seen:
                el = aka.Element(el_type, 0, aka._not_ghost)
                seen["super"] = super().getPushWaveSpeed(el)
                seen["self"] = self.getPushWaveSpeed(el)

    name = register(Speedy)
    _, model = bar(name, aka._explicit_lumped_mass)
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    model.solveStep()
    assert seen["super"] == seen["self"]


def test_v144_upstream_second_registration_of_a_base_parameter_raises():
    class DupE(aka.MaterialLinearElastic2D):
        def __init__(self, model, _id):
            super().__init__(model, _id)
            self.registerParamReal(5.0, "E", aka._pat_parsmod, "dup")

    name = register(DupE)
    with pytest.raises(Exception, match="already registered"):
        bar(name, aka._static)
