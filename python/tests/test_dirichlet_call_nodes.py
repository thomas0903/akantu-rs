# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (no upstream counterpart): Python Dirichlet functors with
# the per-node protocol (`__call__`, upstream) and the optional group
# protocol (`callNodes`, akantu-rs extension) give the same model state.
"""Dirichlet functors: per-node `__call__` and group `callNodes`."""

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


def static_model():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    return mesh, model


def group_nodes(mesh, group):
    return mesh.getElementGroup(group).getNodeGroup().getNodes().flatten()


class PerNode(aka.DirichletFunctor):
    """Blocks x, prescribes a value from the coordinates; records the order."""

    def __init__(self):
        super().__init__()
        self.seen = []

    def __call__(self, node, flags, primal, coord):
        self.seen.append(node)
        primal[0] = 1e-3 * coord[0] + 2e-3 * coord[1]
        primal[1] += 1.0
        flags[0] = True


class Group(aka.DirichletFunctor):
    """The same prescription with one call for the group."""

    def __init__(self):
        super().__init__()
        self.calls = 0

    def callNodes(self, nodes, flags, primal, coords):
        self.calls += 1
        assert nodes.dtype == np.int32 and nodes.ndim == 1
        assert flags.dtype == np.bool_ and flags.shape == (len(nodes), 2)
        assert primal.dtype == np.float64 and primal.shape == (len(nodes), 2)
        assert coords.shape == (len(nodes), 2) and not coords.flags.writeable
        primal[:, 0] = 1e-3 * coords[:, 0] + 2e-3 * coords[:, 1]
        primal[:, 1] += 1.0
        flags[:, 0] = True


class Both(Group):
    """`callNodes` is preferred when both methods exist."""

    def __call__(self, node, flags, primal, coord):
        raise AssertionError("__call__ must not be called")


def state(model):
    return model.getDisplacement().copy(), model.getBlockedDOFs().copy()


def test_group_protocol_matches_per_node_protocol():
    mesh, ref = static_model()
    ref.getDisplacement()[:, 1] = np.arange(mesh.getNbNodes()) * 0.5
    per_node = PerNode()
    ref.applyBC(per_node, "edge")
    assert per_node.seen == list(group_nodes(mesh, "edge"))
    u_ref, b_ref = state(ref)

    for cls in (Group, Both):
        mesh2, model = static_model()
        model.getDisplacement()[:, 1] = np.arange(mesh2.getNbNodes()) * 0.5
        f = cls()
        model.applyBC(f, "edge")
        assert f.calls == 1
        u, b = state(model)
        assert np.array_equal(u.view(np.uint64), u_ref.view(np.uint64))
        assert np.array_equal(b, b_ref)


def test_per_node_views_alias_the_model_rows():
    mesh, model = static_model()
    u = model.getDisplacement()

    class ReadsEarlierWrites(aka.DirichletFunctor):
        def __init__(self):
            super().__init__()
            self.prev = None

        def __call__(self, node, flags, primal, coord):
            if self.prev is not None:
                # the write made for the previous node is visible
                assert u[self.prev, 0] == float(self.prev)
            primal[0] = float(node)
            assert u[node, 0] == float(node)
            self.prev = node

    model.applyBC(ReadsEarlierWrites(), "edge")


def test_group_protocol_errors():
    mesh, model = static_model()

    class Raises(aka.DirichletFunctor):
        def callNodes(self, nodes, flags, primal, coords):
            raise KeyError("boom")

    u0, b0 = state(model)
    with pytest.raises(KeyError, match="boom"):
        model.applyBC(Raises(), "edge")
    # nothing was written back
    u, b = state(model)
    assert np.array_equal(u, u0) and np.array_equal(b, b0)

    class Resizes(aka.DirichletFunctor):
        def callNodes(self, nodes, flags, primal, coords):
            primal.resize((0, 2), refcheck=False)

    with pytest.raises((aka.Exception, ValueError)):
        model.applyBC(Resizes(), "edge")
    # the model is usable afterwards
    model.applyBC(aka.FixedValue(0.0, aka._x), "edge")


def test_empty_group_calls_neither_method():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    mesh.createElementGroup("empty", 2)
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)

    class Counts(aka.DirichletFunctor):
        def __init__(self):
            super().__init__()
            self.calls = 0

        def __call__(self, node, flags, primal, coord):
            self.calls += 1

        def callNodes(self, nodes, flags, primal, coords):
            self.calls += 1

    class PerNodeOnly(aka.DirichletFunctor):
        def __init__(self):
            super().__init__()
            self.calls = 0

        def __call__(self, node, flags, primal, coord):
            self.calls += 1

    u0, b0 = state(model)
    for f in (Counts(), PerNodeOnly()):
        model.applyBC(f, "empty")
        assert f.calls == 0
    u, b = state(model)
    assert np.array_equal(u, u0) and np.array_equal(b, b0)


def test_docstring_describes_both_protocols():
    doc = aka.DirichletFunctor.__doc__
    assert "__call__" in doc and "callNodes" in doc
