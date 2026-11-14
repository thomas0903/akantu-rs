# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests: state lent to a Python
# callback of a running call (python/py_dof_manager.cc
# InterceptSolverCallback / SolverCallback, python/py_material.cc Material,
# self.getModel()) is reachable only from the execution context that runs
# the callback. The greenlet cases are the adversarial programs A1 and A2;
# upstream runs
# them to completion (its handles are plain references), the port refuses
# the use from the other greenlet like a use from another thread (UD-085).
"""Execution contexts of callback lends: greenlets, generators, coroutines,
threads."""

import asyncio
import contextvars
import itertools
import pathlib
import sys
import threading

import numpy as np
import pytest

import akantu as aka
import py11_akantu_test_common as t
from _paths import MESHES, PARSER_FIXTURES

SC = "examples/python/solid_mechanics_model/solver_callback"
CM = "examples/python/solid_mechanics_model/custom-material"
REFUSED = "another execution context"


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
    name = f"local_elastic_ctx_{next(_names)}"
    aka.MaterialFactory.getInstance().registerAllocator(
        name, lambda _d, _u, model, _id: cls(model, _id))
    text = pathlib.Path("material_cm.dat").read_text()
    pathlib.Path(f"{name}.dat").write_text(text.replace("local_elastic", name))
    aka.parseInput(f"{name}.dat")


# --- the check itself (_test_common.lendContextProbe) -----------------------
# A probe records the calling code's context exactly as a lend for a
# callback called from there does; ``admits()`` tells whether a use made now
# would be admitted.


def test_probe_admits_the_lending_call_and_the_code_it_calls():
    def lend():
        p = t.lendContextProbe()

        def nested():
            return p.admits()

        return p, p.admits(), nested()

    p, inside, nested = lend()
    assert (inside, nested) == (True, True)
    assert p.admits() is False  # the call returned


def test_generator_resumed_outside_the_lending_call_is_refused():
    def uses(p):
        while True:
            yield p.admits()

    def lend():
        p = t.lendContextProbe()
        g = uses(p)
        return g, next(g)

    g, inside = lend()
    assert inside is True
    assert next(g) is False


def test_generator_created_outside_and_resumed_inside_is_admitted():
    def asker():
        p = yield
        while True:
            p = yield p.admits()

    g = asker()
    next(g)

    def lend():
        p = t.lendContextProbe()
        return p, g.send(p)

    p, inside = lend()
    assert inside is True
    assert g.send(p) is False


def test_coroutines_and_asyncio_tasks():
    async def check(p):
        await asyncio.sleep(0)
        return p.admits()

    def lend():
        p = t.lendContextProbe()
        inside = asyncio.run(check(p))  # an event loop run inside the call
        c = check(p)
        c.send(None)  # started inside, suspended at the sleep
        return inside, c

    inside, c = lend()
    assert inside is True
    with pytest.raises(StopIteration) as finished:
        c.send(None)  # finished outside
    assert finished.value.value is False

    async def main():
        task = {}

        def lend_in_loop():
            p = t.lendContextProbe()
            task["t"] = asyncio.get_running_loop().create_task(check(p))
            return p.admits()

        assert lend_in_loop() is True
        return await task["t"]  # the task runs after the call returned

    assert asyncio.run(main()) is False


def test_another_thread_is_refused_while_the_lending_call_runs():
    def lend():
        p = t.lendContextProbe()
        out = []
        th = threading.Thread(target=lambda: out.append(p.admits()))
        th.start()
        th.join()
        return out[0], p.admits()

    assert lend() == (False, True)


def test_copied_contextvars_contexts_carry_no_admission():
    def lend():
        p = t.lendContextProbe()
        ctx = contextvars.copy_context()
        return p, ctx, ctx.run(p.admits)

    p, ctx, inside = lend()
    assert inside is True  # run nested inside the call
    assert ctx.run(p.admits) is False


def test_a_patched_sys_getframe_does_not_change_the_answer():
    def lend():
        p = t.lendContextProbe()
        me = sys._getframe()
        out = []

        def other():
            saved = sys._getframe
            sys._getframe = lambda *a: me  # a frame whose chain has the lend
            try:
                out.append(p.admits())
            finally:
                sys._getframe = saved

        th = threading.Thread(target=other)
        th.start()
        th.join()
        return out[0]

    assert lend() is False


def test_greenlet_contexts():
    greenlet = pytest.importorskip("greenlet")
    main = greenlet.getcurrent()
    seen = {}

    def body():
        p = t.lendContextProbe()
        seen["p"] = p
        seen["inside"] = p.admits()
        # a greenlet started inside the lending call starts an empty chain
        seen["child"] = greenlet.greenlet(p.admits).switch()
        main.switch()  # suspended inside the lending call
        seen["resumed"] = p.admits()

    g = greenlet.greenlet(body)
    g.switch()
    seen["main"] = seen["p"].admits()  # the lending call is alive in g
    g.switch()
    del seen["p"]
    assert seen == {"inside": True, "child": False, "main": False,
                    "resumed": True}


# --- the bindings: solver callbacks (program A1) ----------------------------


def eigen_options(method):
    return aka.ModelOptions(
        method,
        aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen),
    )


def bar_model():
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(eigen_options(aka._implicit_dynamic))
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    model.getDisplacement()[:, 0] = 1e-3 * mesh.getNodes()[:, 0]
    model.setTimeStep(model.getStableTimeStep() * 0.64)
    return mesh, model


def scribble(n):
    # C-level recursion (each level re-enters the eval loop through map /
    # sum), so the running greenlet's frames cover the suspended one's
    # stack addresses (program A1)
    return 0 if n == 0 else sum(map(scribble, [n - 1])) + 1


def a1_action(model, action):
    dm = model.getDOFManager()
    if action == "hasMatrix":
        return dm.hasMatrix("K")
    if action == "size":
        return dm.getMatrix("K").size()
    if action == "zero":
        return dm.zeroResidual()
    return model.assembleResidual()


@pytest.mark.parametrize("action", ["hasMatrix", "size", "zero", "assemble"])
def test_a1_greenlet_switch_inside_a_solver_callback(action):
    """A1: a greenlet switch out of ``InterceptSolverCallback.assembleResidual``
    leaves the solve suspended; the other greenlet's use of the running
    solve's access is refused (the port crashed, SIGSEGV), the solve resumes
    and completes."""
    greenlet = pytest.importorskip("greenlet")
    mesh, model = bar_model()
    main = greenlet.getcurrent()
    log = []

    class Switching(aka.InterceptSolverCallback):
        def __init__(self, model):
            super().__init__(model)
            self.model = model
            self.n = 0

        def assembleResidual(self):
            self.n += 1
            if self.n == 1:
                main.switch("suspended in assembleResidual")
                # resumed: this context reaches the running solve again
                log.append(self.model.getDOFManager().hasMatrix("K"))
            self.model.assembleResidual()

    cb = Switching(model)
    g = greenlet.greenlet(lambda: model.solveStep(cb))
    assert g.switch() == "suspended in assembleResidual"
    assert scribble(400) == 400
    with pytest.raises(aka.Exception, match=REFUSED):
        a1_action(model, action)
    assert g.switch() is None  # the solve resumes and completes
    assert g.dead
    assert log == [True]
    assert np.isfinite(model.getDisplacement()).all()
    # after the solve the model is free again
    assert model.getDOFManager().hasMatrix("K")


def test_a1_generator_variant_without_greenlet():
    """A1 with a plain generator (no extra package): a generator that uses
    the running solve is admitted while resumed inside the callback, and
    resumed after the solve it reaches the free model, never the ended
    lend; a generator made before the solve and resumed inside the callback
    is admitted too."""
    mesh, model = bar_model()

    def uses(m):
        while True:
            m.assembleResidual()
            yield m.getDOFManager().getMatrix("K").size()

    before = uses(model)
    kept = []

    class Gen(aka.InterceptSolverCallback):
        def __init__(self, model):
            super().__init__(model)
            self.model = model

        def assembleResidual(self):
            if not kept:
                g = uses(self.model)
                kept.append((g, next(g), next(before)))
            else:
                self.model.assembleResidual()

    model.solveStep(Gen(model))
    g, inside, before_inside = kept[0]
    assert inside == before_inside > 0
    assert next(g) == inside  # the free model, outside any callback
    assert np.isfinite(model.getDisplacement()).all()


def test_a1_thread_variant_without_greenlet():
    """Another thread started inside the callback (which waits for it) is
    refused while the solve's access is lent; the callback and the solve go
    on."""
    mesh, model = bar_model()
    errors = []

    class Threaded(aka.InterceptSolverCallback):
        def __init__(self, model):
            super().__init__(model)
            self.model = model

        def assembleResidual(self):
            if not errors:
                def other():
                    try:
                        self.model.getDOFManager().hasMatrix("K")
                        errors.append(None)
                    except aka.Exception as e:  # noqa: PERF203
                        errors.append(str(e))

                th = threading.Thread(target=other)
                th.start()
                th.join()
            self.model.assembleResidual()

    model.solveStep(Threaded(model))
    assert errors and errors[0] is not None
    assert np.isfinite(model.getDisplacement()).all()


def test_coroutine_driven_inside_a_solver_callback_is_admitted():
    mesh, model = bar_model()
    sizes = []

    async def assemble(m):
        await asyncio.sleep(0)
        m.assembleResidual()
        return m.getDOFManager().getMatrix("K").size()

    class Async(aka.InterceptSolverCallback):
        def __init__(self, model):
            super().__init__(model)
            self.model = model

        def assembleResidual(self):
            sizes.append(asyncio.run(assemble(self.model)))

    model.solveStep(Async(model))
    assert sizes and all(s > 0 for s in sizes)


# --- the bindings: Python materials (program A2) ----------------------------


class LocalElastic(aka.Material):
    """The law of program A2 (upstream custom-material.py)."""

    hook = None

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
        if LocalElastic.hook is not None:
            LocalElastic.hook(self, el_type, ghost_type)
        g = self.getGradU(el_type, ghost_type).reshape((-1, 2, 2))
        s = self.getStress(el_type, ghost_type).reshape((-1, 2, 2))
        eps = 0.5 * (g + np.einsum("aij->aji", g))
        tr = np.einsum("aii->a", g)
        s[:, :, :] = (np.einsum("a,ij->aij", tr, self.lam * np.eye(2))
                      + 2 * self.mu * eps)


@pytest.fixture
def cm_model():
    parse_custom_material(LocalElastic)
    mesh = aka.Mesh(2)
    mesh.read("bar_cm.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    model.getDisplacement()[:, 0] = 1e-3 * np.sin(mesh.getNodes()[:, 0])
    model.setTimeStep(model.getStableTimeStep() * 0.64)
    yield mesh, model
    LocalElastic.hook = None


def test_a2_greenlet_switch_inside_a_python_material(cm_model):
    """A2: a greenlet switch out of ``Material.computeStress``; the other
    greenlet's ``model.getFEEngine()`` (lent model services) and the law's
    own getters (lent law state) are refused (the port read garbage or
    crashed), the solve resumes and completes."""
    greenlet = pytest.importorskip("greenlet")
    mesh, model = cm_model
    main = greenlet.getcurrent()
    state = {"n": 0, "after": None}

    def hook(mat, el_type, ghost_type):
        state["n"] += 1
        if state["n"] == 3:
            state["mat"] = mat
            main.switch("suspended in computeStress")
            state["after"] = mat.getModel().getFEEngine().getNbIntegrationPoints(
                el_type)

    LocalElastic.hook = hook

    def run():
        for _ in range(3):
            model.solveStep()
        return "solve done"

    g = greenlet.greenlet(run)
    assert g.switch() == "suspended in computeStress"
    assert scribble(400) == 400
    with pytest.raises(aka.Exception, match=REFUSED):
        model.getFEEngine().getNbIntegrationPoints(aka._triangle_3)
    with pytest.raises(aka.Exception, match=REFUSED):
        state["mat"].getStress(aka._triangle_3)
    assert g.switch() == "solve done"
    assert state["after"] > 0  # the callback's own context after the resume
    assert np.isfinite(model.getDisplacement()).all()


def test_a2_generator_variant_without_greenlet(cm_model):
    """A2 with a plain generator: a generator made inside ``computeStress``
    reads the lent state there; resumed after the solve it reaches the
    law through the model, never the ended lend."""
    mesh, model = cm_model
    kept = []

    def reads(mat, el_type):
        while True:
            yield (mat.getStress(el_type).shape,
                   mat.getModel().getFEEngine().getNbIntegrationPoints(el_type))

    def hook(mat, el_type, ghost_type):
        if not kept:
            g = reads(mat, el_type)
            kept.append((g, next(g)))

    LocalElastic.hook = hook
    model.solveStep()
    g, inside = kept[0]
    assert next(g) == inside
