# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (group 7, PY-057): python/py_fe_engine.cc
# (computeBtD, computeBtDB, computeNtb, computeNtbN, the ElementTypeMapArray
# overloads of integrate / interpolateOnIntegrationPoints,
# assembleFieldLumped, assembleFieldMatrix), python/py_mesh.cc
# (ElementTypeMapArray.initialize(fe_engine, ...)).
"""FEEngine operators and field assembly."""

import numpy as np
import pytest

import akantu as aka
from _paths import PARSER_FIXTURES, REPO

T = aka._quadrangle_4


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "elastic.dat": PARSER_FIXTURES / "test/test_python_interface/elastic.dat",
            "four.msh": REPO / "crates/akantu-couplers/tests/data/"
            "test_phase_field_model/test_four_elements.msh",
        }
    )


def setup():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("four.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(aka._implicit_dynamic)
    fe = model.getFEEngine()
    nq = fe.getNbIntegrationPoints(T)
    return mesh, model, fe, mesh.getNbElement(element_type=T) * nq, nq


def test_ntb_partition_of_unity_and_ntbn():
    mesh, _, fe, n, nq = setup()
    b = np.column_stack([np.ones(n), np.linspace(-1.0, 2.0, n)])
    ntb = np.zeros((n, 8))
    fe.computeNtb(b, ntb, T)
    shapes = fe.getShapes(T)
    # node-major: Ntb[q, 2 * i + d] = N_i(q) b_d(q)
    np.testing.assert_array_equal(ntb[:, 0::2], shapes)
    np.testing.assert_allclose(ntb[:, 0::2].sum(axis=1), 1.0, rtol=1e-14)
    np.testing.assert_array_equal(ntb[:, 1::2], shapes * b[:, 1:2])
    ntbn = np.zeros((n, 16))
    fe.computeNtbN(np.ascontiguousarray(b[:, 1:2]), ntbn, T)
    for q in range(n):
        expected = np.outer(shapes[q], shapes[q]) * b[q, 1]
        np.testing.assert_allclose(ntbn[q].reshape(4, 4), expected, rtol=1e-15)
    # filtered: two elements
    out = np.zeros((2 * nq, 8))
    fe.computeNtb(np.ascontiguousarray(b[: 2 * nq]), out, T,
                  filter_elements=np.array([[1], [3]]))
    np.testing.assert_array_equal(out[:nq, 0::2], shapes[nq:2 * nq])
    with pytest.raises(aka.Exception):
        fe.computeNtb(b, np.zeros((n, 7)), T)  # wrong output size


def test_btd_btdb():
    mesh, _, fe, n, _ = setup()
    X = mesh.getNodes()
    u = np.column_stack([0.1 * X[:, 0] + 0.2 * X[:, 1], -0.3 * X[:, 0] + 0.05 * X[:, 1]])
    grad = np.zeros((n, 4))
    fe.gradientOnIntegrationPoints(u, grad, 2, T)
    btd = np.zeros((n, 8))
    fe.computeBtD(grad, btd, T)
    dn = fe.getShapesDerivatives(T)  # (n, nn * dim), node-major dN_i/dx_j
    # BtD of a constant D sums to zero over the nodes (sum_i dN_i/dx = 0)
    assert np.all(np.isfinite(btd))
    np.testing.assert_allclose(btd.reshape(n, 4, 2).sum(axis=1), 0.0, atol=1e-12)
    voigt = np.array([[4.0, 1.0, 0.0], [1.0, 4.0, 0.0], [0.0, 0.0, 1.5]])
    btdb = np.zeros((n, 64))
    fe.computeBtDB(np.tile(voigt.ravel(), (n, 1)), btdb, 4, T)
    for q in range(n):
        k = btdb[q].reshape(8, 8)
        np.testing.assert_allclose(k, k.T, rtol=1e-14, atol=1e-14)
        # rigid translations are in the kernel
        np.testing.assert_allclose(k @ np.tile([1.0, 0.0], 4), 0.0, atol=1e-12)
    btdb2 = np.zeros((n, 16))
    fe.computeBtDB(np.tile(np.eye(2).ravel(), (n, 1)), btdb2, 2, T)
    for q in range(n):
        d = dn[q].reshape(4, 2)
        np.testing.assert_allclose(btdb2[q].reshape(4, 4), d @ d.T, rtol=1e-14, atol=1e-14)
    with pytest.raises(aka.Exception):
        fe.computeBtDB(np.tile(voigt.ravel(), (n, 1)), btdb, 3, T)


def test_element_type_map_array_overloads():
    mesh, _, fe, n, nq = setup()
    fq = aka.ElementTypeMapArrayReal()
    fq.initialize(fe, nb_component=2, default_value=1.5)
    assert fq(T).shape == (n, 2) and np.all(fq(T) == 1.5)
    b = np.column_stack([np.linspace(0.0, 1.0, n), np.ones(n)])
    fq(T)[:] = b
    intf = aka.ElementTypeMapArrayReal()
    intf.initialize(mesh, nb_component=2, with_nb_element=True)
    fe.integrate(fq, intf)
    ref = np.zeros((mesh.getNbElement(element_type=T), 2))
    fe.integrate(b, ref, 2, T)
    np.testing.assert_array_equal(intf(T), ref)
    uq = aka.ElementTypeMapArrayReal()
    uq.initialize(fe, nb_component=2)
    u = np.column_stack([mesh.getNodes()[:, 0], 2.0 * mesh.getNodes()[:, 1]])
    fe.interpolateOnIntegrationPoints(u, uq)
    ref = np.zeros((n, 2))
    fe.interpolateOnIntegrationPoints(u, ref, 2, T)
    np.testing.assert_array_equal(uq(T), ref)
    coords = aka.ElementTypeMapArrayReal()
    coords.initialize(fe, nb_component=2)
    fe.computeIntegrationPointsCoordinates(coords)
    np.testing.assert_allclose(uq(T)[:, 0], coords(T)[:, 0], rtol=1e-15)
    with pytest.raises(TypeError):
        fe.integrate(b, intf)


def test_field_assembly():
    mesh, model, fe, n, nq = setup()
    dof = model.getDOFManager()
    seen = []

    def one(mat, element):
        # lumped: the engine's rule (4 points); matrix: the mass rule
        # (order 2 * degree, 9 points), as upstream
        assert mat.shape in ((2, nq), (2, 9)) and np.all(mat == 0.0)
        assert mat.flags["F_CONTIGUOUS"] and mat.flags["WRITEABLE"]
        seen.append(element)
        mat[:, :] = 1.0

    area = fe.integrate(np.ones((n, 1)), T)
    lumped = np.zeros((mesh.getNbNodes(), 2))
    fe.assembleFieldLumped(one, lumped, dof, T)
    assert len(seen) == mesh.getNbElement(element_type=T)
    np.testing.assert_allclose(lumped.sum(axis=0), [area, area], rtol=1e-14)
    lumped2 = lumped.copy()
    fe.assembleFieldLumped(one, lumped2, dof, T)  # accumulates
    np.testing.assert_allclose(lumped2, 2.0 * lumped, rtol=1e-15)

    m2 = dof.getNewMatrix("M2", "M")
    m2.zero()
    fe.assembleFieldMatrix(one, "M2", "displacement", dof, T)
    size = m2.size()
    dense = np.array([[m2(i, j) for j in range(size)] for i in range(size)])
    np.testing.assert_allclose(dense, dense.T, rtol=1e-15)
    np.testing.assert_allclose(dense.sum(), 2.0 * area, rtol=1e-14)
    # consistent mass rows sum to the lumped (row-sum) mass of quadrangles
    np.testing.assert_allclose(dense.sum(axis=1).reshape(-1, 2), lumped, rtol=1e-13)

    def bad(mat, element):
        raise ValueError("no density here")

    with pytest.raises(ValueError, match="no density here"):
        fe.assembleFieldLumped(bad, lumped, dof, T)
    with pytest.raises(ValueError, match="no density here"):
        fe.assembleFieldMatrix(bad, "M2", "displacement", dof, T)

    # a density functor reproduces the model's own masses bitwise (upstream
    # assembles zeros: the functor receives a copy of the matrix, UD-836)
    def rho(mat, element):
        mat[:, :] = 2500.0

    lumped = np.zeros((mesh.getNbNodes(), 2))
    fe.assembleFieldLumped(rho, lumped, dof, T)
    model.assembleMassLumped()
    np.testing.assert_array_equal(lumped, model.getMass())
    m2.zero()
    fe.assembleFieldMatrix(rho, "M2", "displacement", dof, T)
    model.assembleMass()
    mass = dof.getMatrix("M")
    for i in range(size):
        for j in range(size):
            assert m2(i, j) == mass(i, j)

    def resize(mat, element):
        mat.resize((1,), refcheck=False)

    with pytest.raises(aka.Exception, match="keep its shape"):
        fe.assembleFieldLumped(resize, lumped, dof, T)
