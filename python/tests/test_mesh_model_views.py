# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (architecture gate INT-008): NumPy
# views of arrays owned by Mesh / SolidMechanicsModel / materials (zero-copy
# views of the owners' storage, D-047, docs/ARCHITECTURE.md),
# callbacks re-entering Rust while views are held, exceptions crossing the
# boundary, staleness, copy mode.
"""Mesh and model views: writes, staleness, re-entrancy, exceptions."""

import gc

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES


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


def lease(view):
    """The `_ArrayLease` base object of an akantu view."""
    b = view
    while type(b).__name__ != "_ArrayLease":
        b = b.base
    return b


def dcb_mesh():
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    return mesh


def static_model():
    aka.parseInput("elastic.dat")
    mesh = dcb_mesh()
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    return mesh, model


# --- mesh ---------------------------------------------------------------------


def test_mesh_views_shapes_dtypes_and_flags():
    mesh = dcb_mesh()
    nodes = mesh.getNodes()
    assert nodes.shape == (1409, 2) and nodes.dtype == np.float64
    assert nodes.flags.writeable and nodes.flags.c_contiguous
    conn = mesh.getConnectivity(aka._quadrangle_8)
    assert conn.shape == (400, 8) and conn.dtype == np.int32
    group_nodes = mesh.getElementGroup("edge").getNodeGroup().getNodes()
    assert group_nodes.shape == (5, 1) and group_nodes.dtype == np.int32
    # writable as upstream (every `Array &` is cast writable)
    # while no model uses the mesh
    assert group_nodes.flags.writeable
    assert mesh.getConnectivities().elementTypes(dim=1) == [aka._segment_3]


def test_mesh_writes_reach_the_mesh_even_after_the_view_is_dropped():
    mesh = dcb_mesh()
    x0 = mesh.getNodes()[3].copy()
    v = mesh.getNodes()
    v[3] += 1.0
    del v
    gc.collect()
    # the next call applies the writes of the released view
    assert np.array_equal(mesh.getNodes()[3], x0 + 1.0)
    assert np.allclose(mesh.getUpperBounds(), [0.0578, 0.00055])


def test_mesh_resize_makes_old_views_stale_but_safe():
    mesh = dcb_mesh()
    old = mesh.getNodes()
    first = old[0].copy()
    aka.MeshAccessor(mesh).resizeNodes(1500)
    assert lease(old).is_stale()
    assert np.array_equal(old[0], first)  # values at retirement
    old[0] = 42.0  # stale: no effect on the mesh
    new = mesh.getNodes()
    assert new.shape == (1500, 2) and not lease(new).is_stale()
    assert np.array_equal(new[0], first)
    assert np.array_equal(new[1499], [0.0, 0.0])


def test_mesh_view_sees_later_changes_without_refetch():
    mesh = aka.Mesh(2)
    acc = aka.MeshAccessor(mesh)
    acc.resizeNodes(3)
    nodes = mesh.getNodes()
    nodes[:] = [[0, 0], [1, 0], [0, 1]]
    mesh.addConnectivityType(aka._triangle_3)
    acc.resizeConnectivity(1, aka._triangle_3)
    conn = mesh.getConnectivity(aka._triangle_3)
    assert conn.tolist() == [[-1, -1, -1]]  # upstream fills with -1
    conn[0] = [0, 1, 2]
    acc.makeReady()
    assert mesh.getConnectivity(aka._triangle_3).tolist() == [[0, 1, 2]]
    assert np.allclose(mesh.getUpperBounds(), [1, 1])


def test_mesh_cannot_be_modified_once_a_model_uses_it():
    mesh, model = static_model()
    with pytest.raises(aka.Exception, match="used by a model"):
        mesh.createNodeGroup("late", False)
    # reading still works
    assert mesh.getNbNodes() == 1409
    del model


def test_element_type_map_array_owned_and_referenced():
    mesh = dcb_mesh()
    e = aka.ElementTypeMapArrayReal()
    e.initialize(mesh, nb_component=2, with_nb_element=True,
                 default_value=0.5)
    assert e.getID() == "no_parent:by_element_type_array"
    a = e(aka._quadrangle_8)
    assert a.shape == (400, 2) and np.all(a == 0.5)
    a[0, 0] = 3.0
    assert e(aka._quadrangle_8)[0, 0] == 3.0  # owned: native zero-copy
    with pytest.raises(aka.Exception):
        mesh.getConnectivities().initialize(mesh)


# --- model ---------------------------------------------------------------------


def test_model_views_are_writable_and_reach_the_model():
    mesh, model = static_model()
    disp = model.getDisplacement()
    assert disp.shape == (1409, 2) and disp.dtype == np.float64
    blocked = model.getBlockedDOFs()
    assert blocked.dtype == np.bool_
    disp[:, 0] = 1e-3  # rigid translation: no internal force
    blocked[:] = True
    model.assembleInternalForces()
    assert np.abs(model.getInternalForce()).max() < 1e-6
    disp[:, 0] = 0.0
    disp[5, 0] = 1e-3
    model.assembleInternalForces()
    f = model.getInternalForce()
    assert np.abs(f).max() > 1.0


def test_a_held_view_sees_the_values_after_each_call():
    mesh, model = static_model()
    disp = model.getDisplacement()
    model.applyBC(aka.FixedValue(0.25, aka._y), "edge")
    nodes = mesh.getElementGroup("edge").getNodeGroup().getNodes().ravel()
    assert np.all(disp[nodes, 1] == 0.25)  # no refetch needed
    assert not lease(disp).is_stale()


def test_static_solve_with_python_bcs_and_energies():
    mesh, model = static_model()
    blocked = model.getBlockedDOFs()
    disp = model.getDisplacement()
    left = mesh.getNodes()[:, 0] < 1e-12
    right = mesh.getNodes()[:, 0] > 0.0578 - 1e-12
    blocked[left, :] = True
    blocked[right, 0] = True
    disp[right, 0] = 1e-5
    model.solveStep()
    assert np.allclose(disp[right, 0], 1e-5)
    epot = model.getEnergy("potential")
    assert epot > 0
    stress = model.getMaterial(0).getStress(aka._quadrangle_8)
    assert stress.shape == (400 * 4, 4)  # 2x2 Gauss points (Q8)
    assert stress[:, 0].mean() > 0  # tension along x


def test_non_convergence_raises_nls_not_converged():
    mesh, model = static_model()
    blocked = model.getBlockedDOFs()
    blocked[mesh.getNodes()[:, 0] < 1e-12, :] = True
    model.getExternalForce()[100, 1] = 1.0
    solver = model.getNonLinearSolver()
    solver.set("max_iterations", 1)
    solver.set("threshold", 1e-300)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)
    with pytest.raises(aka.NLSNotConvergedException):
        model.solveStep()


def test_callback_reenters_rust_while_views_are_held():
    """A Python functor called from inside `applyBC` (native section) reads
    a view it holds, calls into another akantu object (the mesh), and gets
    live views from the busy model's array getters;
    other calls on the busy model raise with an explicit message."""
    mesh, model = static_model()
    disp = model.getDisplacement()
    disp[:, 1] = 7.0
    seen = {}

    class Probe(aka.DirichletFunctor):
        def __call__(self, node, flags, primal, coord):
            seen["held"] = float(disp[node, 1])  # held view: readable
            seen["nb_nodes"] = mesh.getNbNodes()  # other object: fine
            inner = model.getDisplacement()  # busy model: live view
            seen["same_memory"] = np.shares_memory(inner, disp)
            try:
                model.getEnergy("potential")
            except aka.Exception as e:
                seen["reentry"] = str(e)
            primal[0] = 2.0
            flags[0] = True

    model.applyBC(Probe(), "edge")
    assert seen["held"] == 7.0
    assert seen["nb_nodes"] == 1409
    assert seen["same_memory"]
    assert "in use by the running call" in seen["reentry"]
    nodes = mesh.getElementGroup("edge").getNodeGroup().getNodes().ravel()
    assert np.all(disp[nodes, 0] == 2.0) and np.all(disp[nodes, 1] == 7.0)


def test_python_exception_in_functor_keeps_its_type():
    mesh, model = static_model()

    class Boom(aka.NeumannFunctor):
        def __call__(self, quad_point, dual, coord, normals):
            assert isinstance(quad_point, aka.IntegrationPoint)
            assert not coord.flags.writeable
            raise KeyError("boom")

    with pytest.raises(KeyError, match="boom"):
        model.applyBC(Boom(), "edge")
    # the model is usable afterwards
    model.applyBC(aka.FromSameDim(np.array([1.0, 0.0])), "edge")
    assert model.getExternalForce()[:, 0].sum() > 0


def test_akantu_errors_map_to_akantu_exception():
    mesh, model = static_model()
    with pytest.raises(aka.Exception):
        model.applyBC(aka.FixedValue(0.0, aka._x), "no_such_group")
    with pytest.raises(aka.Exception):
        model.getMaterial("no_such_material")
    with pytest.raises(aka.Exception):
        aka.DirichletFunctor()  # base functor: no __call__
        model.applyBC(aka.DirichletFunctor(), "edge")


def test_material_parameters_and_internals():
    mesh, model = static_model()
    mat = model.getMaterial(0)
    assert isinstance(mat, aka.Material) and isinstance(mat, aka.Parsable)
    assert mat.getReal("E") == 71e9 and mat.getReal("nu") == 0.22
    mat.setReal("E", 70e9)
    assert model.getMaterial("bulk").getReal("E") == 70e9
    grad_u = mat.getGradU(aka._quadrangle_8)
    assert grad_u.shape == (1600, 4)
    filt = mat.getElementFilter()(aka._quadrangle_8)
    # read-only snapshot copy (D-056 item 2, UD-747; upstream: a writable
    # view of the law's filter)
    assert filt.shape == (400, 1) and not filt.flags.writeable
    assert np.array_equal(filt.ravel(), np.arange(400))
    by_el = model.getMaterialByElement()(aka._quadrangle_8)
    assert by_el.shape == (400, 1) and np.all(by_el == 0)


def test_copy_mode_returns_copies_of_model_arrays():
    mesh, model = static_model()
    aka.setArrayCopyMode(True)
    try:
        d = model.getDisplacement()
        d[:] = 1.0
        assert np.all(model.getDisplacement() == 0.0)
    finally:
        aka.setArrayCopyMode(False)
    d = model.getDisplacement()
    d[:] = 1.0
    assert np.all(model.getDisplacement() == 1.0)


def test_initfull_with_materials_dict():
    mesh = dcb_mesh()
    aka.parseInput("elastic.dat")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(
        {"soft": {"type": "elastic", "E": 1e6, "nu": 0.3, "rho": 1.0}},
        _analysis_method=aka._static,
    )
    assert model.getNbMaterials() == 2  # parsed "bulk" + "soft"
    assert model.getMaterial("soft").getReal("E") == 1e6


# --- FE engine -------------------------------------------------------------------


def test_fe_engine_patch_and_outputs():
    mesh, model = static_model()
    fe = model.getFEEngine()
    t = aka._quadrangle_8
    nq = fe.getNbIntegrationPoints(t)
    ne = mesh.getNbElement(element_type=t)
    X = mesh.getNodes()
    u = np.column_stack([1 + 2 * X[:, 0] + 3 * X[:, 1], 4 - X[:, 0]])
    uq = np.zeros((ne * nq, 2))
    fe.interpolateOnIntegrationPoints(u, uq, 2, t)
    xq = np.zeros((ne * nq, 2))
    fe.interpolateOnIntegrationPoints(X, xq, 2, t)
    assert np.allclose(uq[:, 0], 1 + 2 * xq[:, 0] + 3 * xq[:, 1], atol=1e-14)
    grad = np.zeros((ne * nq, 4))
    fe.gradientOnIntegrationPoints(u, grad, 2, t)
    # per point a column-major (nb_dof x dim) matrix (upstream layout):
    # du0/dx, du1/dx, du0/dy, du1/dy
    assert np.allclose(grad, [2, -1, 3, 0], atol=1e-10)
    area = fe.integrate(np.ones((ne * nq, 1)), t)
    assert np.isclose(area, 0.0578 * 0.0011, rtol=1e-12)
    with pytest.raises(aka.Exception, match="values"):
        fe.integrate(np.ones((ne * nq, 1)), np.zeros((3, 1)), 1, t)
    # an akantu view as output: written in place; stale views refused
    owned = aka.ElementTypeMapArrayReal()
    owned.initialize(mesh, nb_component=1, with_nb_element=True)
    target = owned(t)
    fe.integrate(np.ones((ne * nq, 1)), target, 1, t)
    assert np.isclose(owned(t).sum(), area, rtol=1e-12)
    owned.initialize(mesh, nb_component=1)  # shrinks the blocks to 0
    assert lease(target).is_stale()
    with pytest.raises(aka.Exception, match="stale"):
        fe.integrate(np.ones((ne * nq, 1)), target, 1, t)
    assert fe.getShapes(t).flags.writeable is False
    assert isinstance(model.getFEEngineBoundary(), aka.FEEngine)
