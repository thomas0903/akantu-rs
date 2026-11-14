# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs regression tests (decision record D-047): the
# adversarial programs (cc1_p*.py, py_p10_*.py) as tests asserting the upstream
# results. Upstream semantics: every `Array &` getter returns a NumPy view
# aliasing the C++ array (python/py_aka_array.hh `aka_array_cast(src,
# none())`), boundary-condition functors get row views of the live arrays
# (python/py_boundary_conditions.cc, `VectorProxy` through pybind11's Eigen
# caster). Each test failed on the earlier mirror implementation.
"""Zero-copy views: writes and reads inside callbacks, no end-of-call
synchronisation, mesh writes after a model exists, re-entrant getters,
Python callables inside a solve step."""

import gc
import threading
import time

import numpy as np
import pytest

import akantu as aka
import py11_akantu_test_common as tc
from _paths import MESHES, PARSER_FIXTURES

DIM = 2


@pytest.fixture(autouse=True)
def _dcb(upstream_dir):
    upstream_dir(
        **{
            "mesh_dcb_2d.msh": MESHES
            / "test/test_python_interface/mesh_dcb_2d.msh",
            "elastic.dat": PARSER_FIXTURES
            / "test/test_python_interface/elastic.dat",
        }
    )


def _model(method):
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(DIM)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=method)
    return mesh, model


def static_model():
    return _model(aka._static)


def explicit_model():
    return _model(aka._explicit_lumped_mass)


def edge_nodes(mesh, group="edge"):
    return [int(n) for n in
            mesh.getElementGroup(group).getNodeGroup().getNodes().ravel()]


def bits(a):
    return np.array(a, dtype=np.float64).view(np.uint64)


def lease(view):
    b = view
    while type(b).__name__ != "_ArrayLease":
        b = b.base
    return b


# --- P1: in-place writes followed by solves (kept bitwise) ----------------


D1, D2 = 0.5e-5, 1e-5


def _p1_native(steps):
    mesh, model = static_model()
    model.applyBC(aka.FixedValue(0.0, aka._x), "edge")
    model.applyBC(aka.FixedValue(0.0, aka._y), "edge")
    res = []
    for d in steps:
        model.applyBC(aka.FixedValue(d, aka._x), "coh")
        model.solveStep()
        res.append(np.array(model.getDisplacement(), copy=True))
    return res


def _p1_temp(steps):
    mesh, model = static_model()
    e, c = edge_nodes(mesh, "edge"), edge_nodes(mesh, "coh")
    model.getBlockedDOFs()[e, :] = True
    model.getBlockedDOFs()[c, 0] = True
    res = []
    for d in steps:
        model.getDisplacement()[c, 0] = d
        model.solveStep()
        res.append(np.array(model.getDisplacement(), copy=True))
    return res


def _p1_retained(steps):
    mesh, model = static_model()
    e, c = edge_nodes(mesh, "edge"), edge_nodes(mesh, "coh")
    u = model.getDisplacement()
    b = model.getBlockedDOFs()
    b[e, :] = True
    b[c, 0] = True
    res, held = [], []
    prev = 0.0
    for d in steps:
        u[c, 0] += d - prev
        prev = d
        model.solveStep()
        held.append(np.array(u, copy=True))
        res.append(np.array(model.getDisplacement(), copy=True))
    return res, held


def _p1_dropped_and_derived(steps):
    mesh, model = static_model()
    e, c = edge_nodes(mesh, "edge"), edge_nodes(mesh, "coh")
    b = model.getBlockedDOFs()
    flat_b = b.reshape(-1)
    for n in e:
        flat_b[2 * n] = True
        flat_b[2 * n + 1] = True
    for n in c:
        flat_b[2 * n] = True
    del b, flat_b
    gc.collect()
    res = []
    for d in steps:
        u = model.getDisplacement()
        col = u[:, 0]
        vals = np.array(col, copy=True)
        vals[c] = d
        np.copyto(col, vals)
        del u, col
        gc.collect()
        model.solveStep()
        res.append(np.array(model.getDisplacement(), copy=True))
    return res


def test_p1_inplace_writes_then_solves_bitwise_vs_native_functor():
    steps = [D1, D2]
    ref = _p1_native(steps)
    ret, held = _p1_retained(steps)
    for name, res in [("temp", _p1_temp(steps)),
                      ("dropped_derived", _p1_dropped_and_derived(steps)),
                      ("retained", ret), ("retained_held", held)]:
        for i, r in enumerate(res):
            assert np.array_equal(bits(r), bits(ref[i])), (name, i)


# --- P2: writes from inside BC functors are kept -------------------


def test_p2a_dirichlet_functor_writes_into_retained_velocity_view():
    mesh, model = explicit_model()
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    vel = model.getVelocity()
    nodes = edge_nodes(mesh)

    class FixAndKick(aka.DirichletFunctor):
        def __call__(self, node, flags, primal, coord):
            primal[0] = 0.0
            flags[0] = True
            vel[node, 1] = 3.0

    model.applyBC(FixAndKick(), "edge")
    assert np.all(vel[nodes, 1] == 3.0)
    for _ in range(5):
        model.solveStep()
    assert np.all(np.array(model.getDisplacement())[nodes, 1] > 0.0)


def test_p2b_neumann_functor_accumulates_into_retained_force_view():
    mesh, model = static_model()
    fext = model.getExternalForce()
    calls = []

    class TractionAndTally(aka.NeumannFunctor):
        def __call__(self, quad_point, dual, coord, normals):
            dual[:] = [1.0, 0.0]
            calls.append(1)
            fext[0, 1] += 1.0

    model.applyBC(TractionAndTally(), "edge")
    assert calls and float(fext[0, 1]) == float(len(calls))


def test_p2c_dirichlet_functor_writes_another_nodes_displacement():
    mesh, model = static_model()
    u = model.getDisplacement()
    other = edge_nodes(mesh, "coh")[-1]
    assert other not in edge_nodes(mesh)

    class FixAndPull(aka.DirichletFunctor):
        def __call__(self, node, flags, primal, coord):
            primal[:] = 0.0
            flags[:] = True
            u[other, 0] = 1e-6

    model.applyBC(FixAndPull(), "edge")
    assert float(u[other, 0]) == 1e-6


# --- P3: reads inside a functor see earlier writes of the same call --------


def test_p3_functor_sees_values_written_earlier_in_the_same_call():
    mesh, model = static_model()
    u = model.getDisplacement()
    order = []

    class Ramp(aka.DirichletFunctor):
        def __call__(self, node, flags, primal, coord):
            prev = order[-1] if order else None
            base = float(u[prev, 0]) if prev is not None else 0.0
            primal[0] = base + 1e-6
            flags[0] = True
            order.append(int(node))

    model.applyBC(Ramp(), "edge")
    acc, expected = 0.0, []
    for _ in order:
        acc = acc + 1e-6
        expected.append(acc)
    assert np.array_equal(np.array([u[n, 0] for n in order]),
                          np.array(expected))


def test_dirichlet_functor_arguments_alias_the_live_rows():
    """`primal`/`flags` are row views of the model arrays and `coord` a
    read-only row view of the mesh nodes, as upstream."""
    mesh, model = static_model()
    u = model.getDisplacement()
    b = model.getBlockedDOFs()
    x = mesh.getNodes()
    seen = []

    class Alias(aka.DirichletFunctor):
        def __call__(self, node, flags, primal, coord):
            primal[1] = 5e-7
            seen.append((float(u[node, 1]), np.array_equal(coord, x[node]),
                         coord.flags.writeable, primal.shape, flags.dtype))
            flags[1] = True
            seen.append(bool(b[node, 1]))

    model.applyBC(Alias(), "edge")
    assert seen
    for a, written in zip(seen[0::2], seen[1::2]):
        assert a == (5e-7, True, False, (2,), np.bool_)
        assert written


# --- P4: no state-changing work at call boundaries -----------------


def test_p4a_current_position_includes_a_python_write():
    mesh, model = static_model()
    u = model.getDisplacement()
    model.getBlockedDOFs()[edge_nodes(mesh), :] = True
    model.solveStep()
    u[:, 0] += 1e-4
    cp = np.array(model.getCurrentPosition(), copy=True)
    expect = np.array(mesh.getNodes()) + np.array(u)
    assert np.array_equal(cp, expect)


def test_p4b_stable_time_step_uses_a_python_written_displacement():
    mesh, model = explicit_model()
    u = model.getDisplacement()
    model.setTimeStep(model.getStableTimeStep() * 0.1)
    model.solveStep()
    x = np.array(mesh.getNodes())
    u[:, :] = -0.5 * x
    dt = model.getStableTimeStep()
    aka.parseInput("elastic.dat")
    mesh2 = aka.Mesh(DIM)
    mesh2.read("mesh_dcb_2d.msh")
    mesh2.getNodes()[:, :] = 0.5 * np.array(mesh2.getNodes())
    model2 = aka.SolidMechanicsModel(mesh2)
    model2.initFull(_analysis_method=aka._explicit_lumped_mass)
    assert dt == model2.getStableTimeStep()


def test_a_held_view_is_never_rewritten_by_an_unrelated_call():
    mesh, model = static_model()
    f = model.getExternalForce()
    f[3, 1] = 42.0
    model.getNbMaterials()
    repr(model)
    model.getMaterial(0).getReal("E")
    assert f[3, 1] == 42.0
    assert model.getExternalForce()[3, 1] == 42.0


# --- P5: mesh-view writes after a model exists ---------------------


def test_p5_node_write_after_model_lands_and_nothing_breaks():
    mesh, model = static_model()
    model.getBlockedDOFs()[edge_nodes(mesh), :] = True
    nodes = mesh.getNodes()
    x0 = float(nodes[0, 0])
    nodes[0, 0] += 1e-9
    assert mesh.getNodes()[0, 0] == x0 + 1e-9  # landed in the mesh
    model.solveStep()
    model.getEnergy("potential")
    assert mesh.getNbNodes() == 1409
    del nodes
    gc.collect()
    model.solveStep()
    assert mesh.getNbNodes() == 1409
    assert mesh.getNodes()[0, 0] == x0 + 1e-9
    # the model sees the new geometry where it reads it (upstream:
    # current position = X + u)
    assert model.getCurrentPosition()[0, 0] == (
        mesh.getNodes()[0, 0] + model.getDisplacement()[0, 0])


def test_topology_views_go_read_only_and_earlier_ones_stale_with_a_model():
    """UD-532: once a model uses the mesh, connectivity and group-list views
    are read-only and views fetched before are stale (their writes no longer
    reach the mesh); node coordinates stay writable."""
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(DIM)
    mesh.read("mesh_dcb_2d.msh")
    conn = mesh.getConnectivity(aka._quadrangle_8)
    group = mesh.getElementGroup("edge").getNodeGroup().getNodes()
    assert conn.flags.writeable and group.flags.writeable
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    assert lease(conn).is_stale() and lease(group).is_stale()
    before = int(conn[0, 0])
    conn[0, 0] = 10**6  # into the retired block: harmless
    fresh = mesh.getConnectivity(aka._quadrangle_8)
    assert not fresh.flags.writeable and int(fresh[0, 0]) == before
    assert not mesh.getElementGroup("edge").getNodeGroup().getNodes().flags.writeable
    assert mesh.getNodes().flags.writeable


def test_out_of_range_topology_written_from_python_is_refused_at_model_creation():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(DIM)
    mesh.read("mesh_dcb_2d.msh")
    mesh.getConnectivity(aka._quadrangle_8)[0, 0] = 99999
    with pytest.raises(aka.Exception, match="invalid node index"):
        aka.SolidMechanicsModel(mesh)


# --- P6: another thread writes during a callback ---------------------------


def test_p6_write_from_another_thread_during_a_callback_is_kept():
    mesh, model = static_model()
    fext = model.getExternalForce()
    inside = threading.Event()
    written = threading.Event()

    def writer():
        inside.wait(10)
        fext[7, 1] = 123.0
        written.set()

    class Slow(aka.DirichletFunctor):
        def __call__(self, node, flags, primal, coord):
            primal[0] = 0.0
            flags[0] = True
            if not inside.is_set():
                inside.set()
                written.wait(10)
                time.sleep(0.01)

    t = threading.Thread(target=writer)
    t.start()
    model.applyBC(Slow(), "edge")
    t.join()
    assert written.is_set() and float(fext[7, 1]) == 123.0


# --- P8: parameter setters act on the live internals ---------------


def test_p8_setreal_discards_an_eigenstrain_written_through_a_view():
    mesh, model = static_model()
    model.getBlockedDOFs()[edge_nodes(mesh), :] = True
    mat = model.getMaterial(0)
    eg = mat.getInternalReal("eigen_grad_u")(aka._quadrangle_8)
    eg[:, 0] = 1e-3
    mat.setReal("E", mat.getReal("E"))  # upstream: resets eigen_grad_u
    assert np.all(eg == 0.0)  # the held view shows the reset at once
    model.solveStep()
    u = np.array(model.getDisplacement())
    after = np.array(mat.getInternalReal("eigen_grad_u")(aka._quadrangle_8))
    assert np.abs(u).max() == 0.0 and np.all(after == 0.0)


def test_eigenstrain_written_through_a_view_drives_the_solve():
    mesh, model = static_model()
    model.getBlockedDOFs()[edge_nodes(mesh), :] = True
    eg = model.getMaterial(0).getInternalReal("eigen_grad_u")(aka._quadrangle_8)
    eg[:, 0] = 1e-3
    model.solveStep()
    assert np.abs(np.array(model.getDisplacement())).max() > 0.0


# --- / foundation: model access inside callbacks --------------


def test_model_array_getters_inside_a_functor_return_live_views():
    mesh, model = explicit_model()
    seen = {}
    model.getVelocity()  # exported once; the others were never fetched

    class Inside(aka.DirichletFunctor):
        def __call__(self, node, flags, primal, coord):
            u = model.getDisplacement()
            v = model.getVelocity()
            f = model.getExternalForce()
            b = model.getBlockedDOFs()
            primal[0] = 1e-7
            seen.setdefault("u_live", float(u[node, 0]) == 1e-7)
            v[node, 1] = -1.0
            f[node, 0] = 2.0
            seen["blocked_before"] = bool(b[node, 0])
            flags[0] = True

    model.applyBC(Inside(), "edge")
    nodes = edge_nodes(mesh)
    assert seen["u_live"]
    assert np.all(model.getVelocity()[nodes, 1] == -1.0)
    assert np.all(model.getExternalForce()[nodes, 0] == 2.0)
    assert not seen["blocked_before"]


def test_python_callable_inside_a_solve_step_reads_and_writes_model_arrays():
    """A Python callable invoked from inside the solver (before each residual
    assembly) reads model arrays and writes a load; the result is bitwise
    the solve with the load applied beforehand."""
    top = 1000

    def setup():
        mesh, model = static_model()
        model.getBlockedDOFs()[edge_nodes(mesh), :] = True
        return mesh, model

    mesh, ref = setup()
    ref.getExternalForce()[top, 1] = 1.0
    ref.solveStep()
    expected = np.array(ref.getDisplacement(), copy=True)

    mesh, model = setup()
    seen = []

    def hook():
        u = model.getDisplacement()  # getter inside the solve step
        seen.append(np.array(u[top], copy=True))
        model.getExternalForce()[top, 1] = 1.0  # write picked up now

    calls = tc.solveStepWithHook(model, hook, "assemble_residual")
    assert calls >= 1 and len(seen) == calls
    assert np.array_equal(bits(model.getDisplacement()), bits(expected))
    assert np.all(seen[0] == 0.0)  # first assembly: before the update


def test_solve_step_hook_sees_the_predictor_update_live():
    mesh, model = explicit_model()
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    v = model.getVelocity()
    v[:, 0] = 1.0
    seen = []

    def hook():
        seen.append(float(model.getDisplacement()[0, 0]))

    tc.solveStepWithHook(model, hook, "assemble_residual")
    # the explicit predictor moved u by dt * v before the residual assembly
    assert seen and seen[0] > 0.0


# ---: reference getters are writable views ---------------------------


def test_reference_getters_return_writable_views_like_upstream():
    mesh, model = explicit_model()
    model.assembleMassLumped()
    mat = model.getMaterial(0)
    for a in [model.getMass(), model.getCurrentPosition(),
              model.getPreviousDisplacement(), model.getDisplacement(),
              model.getIncrement(), model.getInternalForce(),
              model.getMaterialByElement()(aka._quadrangle_8),
              mat.getStress(aka._quadrangle_8), mat.getGradU(aka._quadrangle_8)]:
        assert a.flags.writeable
        assert not lease(a).is_stale()
    m = model.getMass()
    m[0, 0] = 7.0
    assert model.getMass()[0, 0] == 7.0


def test_v62_a3_other_thread_calls_during_a_callback_raise():
    # program A3, UD-085 / UD-535: while the main thread
    # is inside `applyBC` and its Python functor releases the GIL, another
    # thread's NumPy write into a held view is kept (as upstream), but its
    # akantu calls raise `akantu.Exception` (upstream runs them
    # unsynchronised): the runtime admits one thread at a time
    mesh, model = explicit_model()
    fext = model.getExternalForce()
    ev_in, ev_out = threading.Event(), threading.Event()
    res = {}

    def outcome(f):
        try:
            f()
            return "ok"
        except aka.Exception:
            return "akantu.Exception"

    def other():
        ev_in.wait(30)
        fext[8, 1] = 123.0
        res["model_getter"] = outcome(lambda: model.getDisplacement())
        res["mesh_getter"] = outcome(lambda: mesh.getNbNodes())
        res["model_call"] = outcome(lambda: model.getStableTimeStep())
        ev_out.set()

    class Wait(aka.DirichletFunctor):
        def __init__(self):
            super().__init__()
            self.first = True

        def __call__(self, node, flags, primal, coord):
            if self.first:
                self.first = False
                ev_in.set()
                ev_out.wait(30)

    th = threading.Thread(target=other)
    th.start()
    model.applyBC(Wait(), "edge")
    th.join(60)
    assert not th.is_alive()
    assert res == {
        "model_getter": "akantu.Exception",
        "mesh_getter": "akantu.Exception",
        "model_call": "akantu.Exception",
    }
    assert model.getExternalForce()[8, 1] == 123.0


def test_ud747_element_filter_is_a_read_only_snapshot_copy():
    # D-056 item 2: a law's element filter is not exportable storage;
    # `getElementFilter()(type)` returns a read-only copy of its values
    # (upstream: a writable view, whose writes corrupt the law)
    mesh, model = explicit_model()
    mat = model.getMaterial(0)
    filt = mat.getElementFilter()
    a = filt(aka._quadrangle_8)
    b = filt(aka._quadrangle_8)
    assert a.dtype == np.int32 and a.shape == (400, 1)
    assert not a.flags.writeable
    assert np.array_equal(a, b) and not np.shares_memory(a, b)
    with pytest.raises(ValueError):
        a[0, 0] = 3
    model.assembleMassLumped()
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    model.solveStep()
    assert np.array_equal(filt(aka._quadrangle_8), b)


# --- P10: views outlive their owners (stale-but-safe) ----------------------


def test_p10_views_outlive_model_and_mesh_stale_but_safe():
    mesh, model = static_model()
    u = model.getDisplacement()
    x = mesh.getNodes()
    s = model.getMaterial(0).getStress(aka._quadrangle_8)
    u[:, 0] = 1.0
    model.assembleInternalForces()
    del model, mesh
    gc.collect()
    assert lease(u).is_stale() and np.all(u[:, 0] == 1.0)
    assert x.shape == (1409, 2) and np.isfinite(s).all()
    u[:] = 5.0  # into the retired block: harmless
    assert np.all(u == 5.0)


# --- upstream defects of the bindings registered by --------------


def test_ud533_get_fe_engine_boundary_returns_the_boundary_engine():
    mesh, model = static_model()
    boundary = model.getFEEngineBoundary()
    normals = boundary.getNormalsOnIntegrationPoints(aka._segment_3)
    assert normals.shape[1] == 2
    assert np.allclose(np.linalg.norm(normals, axis=1), 1.0)
    with pytest.raises(aka.Exception):
        model.getFEEngine().getNormalsOnIntegrationPoints(aka._segment_3)


def test_ud536_fe_engine_arrays_are_read_only_copies():
    mesh, model = static_model()
    shapes = model.getFEEngine().getShapes(aka._quadrangle_8)
    with pytest.raises(ValueError):
        shapes[0, 0] = 1.0
