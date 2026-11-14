# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests: the small Python-interface
# behaviours of PY-007 (version), PY-052 (`createClusters` with the default
# `ClusteringFilter`, ported from python/py_group_manager.cc:175-184), PY-056
# (`Element` / `ElementNull`, python/py_fe_engine.cc:41-51, UD-021 corrected
# keywords) and PY-096 (`initialize` / `finalize` stubs and the
# `FromStress` / `FromTraction` aliases of python/akantu/__init__.py).
"""Version, clusters, Element keywords, stubs and BC aliases."""

import re
import warnings

import pytest

import akantu as aka
from _paths import MESHES, REPO

UPSTREAM_VERSION = "v5.0.7-post1-554-g35a851780"


def test_version_format_port_plus_upstream():  # PY-007
    version = aka.getVersion()
    assert aka.__version__ == version
    m = re.fullmatch(r"(\d+\.\d+\.\d+(?:-[0-9A-Za-z.\-]+)?)\+akantu\.(.+)", version)
    assert m, version
    assert m.group(2) == UPSTREAM_VERSION
    # the port part is the workspace version of Cargo.toml
    cargo = (REPO / "Cargo.toml").read_text()
    workspace = re.search(r'\[workspace\.package\][^\[]*?\nversion = "([^"]+)"', cargo)
    assert workspace and m.group(1) == workspace.group(1)


@pytest.fixture
def dcb_mesh(upstream_dir):
    upstream_dir(
        **{"mesh_dcb_2d.msh": MESHES / "test/test_python_interface/mesh_dcb_2d.msh"}
    )
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    return mesh


def test_create_clusters_with_the_default_filter(dcb_mesh):  # PY-052
    mesh = dcb_mesh
    before = {g.getName() for g in mesh.iterateElementGroups()}
    nb_2d = mesh.getNbElement(spatial_dimension=2)
    assert nb_2d == 400

    n = mesh.createClusters(2, "cl", aka.ClusteringFilter())
    assert isinstance(n, int) and n >= 1
    names = [f"cl_{i}" for i in range(n)]  # upstream naming `<prefix>_<i>`
    after = {g.getName() for g in mesh.iterateElementGroups()}
    assert after - before == set(names)
    # the default filter accepts every element: the clusters partition them
    assert sum(len(mesh.getElementGroup(c)) for c in names) == nb_2d

    # the prefix and filter arguments are positional in upstream; a
    # non-filter object is rejected, not silently ignored
    with pytest.raises(TypeError):
        mesh.createClusters(2, "bad", object())


def test_element_constructor_and_corrected_keywords():  # PY-056, UD-021
    e = aka.Element(aka._quadrangle_4, 7)
    assert e.type == aka._quadrangle_4
    assert e.element == 7
    assert e.ghost_type == aka._not_ghost
    # positional ghost type; and the corrected keyword names
    g = aka.Element(aka._triangle_3, 3, aka._ghost)
    assert (g.type, g.element, g.ghost_type) == (aka._triangle_3, 3, aka._ghost)
    k = aka.Element(type=aka._triangle_3, element=3, ghost_type=aka._ghost)
    assert (k.type, k.element, k.ghost_type) == (aka._triangle_3, 3, aka._ghost)
    assert isinstance(repr(e), str) and repr(e) != repr(g)
    # upstream defines only `__lt__`; equality is identity
    assert aka.Element(aka._triangle_3, 3) < aka.Element(aka._triangle_3, 4)
    assert not (aka.Element(aka._triangle_3, 4) < aka.Element(aka._triangle_3, 3))
    assert e != aka.Element(aka._quadrangle_4, 7)
    # the null element
    null = aka.ElementNull
    assert isinstance(null, aka.Element)
    assert null.element == -1
    assert null.type == aka._not_defined
    assert aka.Element(aka._not_defined, 5).element == -1  # canonical null
    assert aka.Element(aka._triangle_3, -1).element == -1


def test_initialize_finalize_stubs():  # PY-096
    with pytest.raises(RuntimeError, match="No need to call initialize"):
        aka.initialize()
    with pytest.raises(RuntimeError, match="No need to call initialize"):
        aka.initialize("--aka_input_file", "x.dat")
    with pytest.warns(DeprecationWarning, match="No need to call finalize"):
        assert aka.finalize() is None
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        aka.finalize(1, key=2)
    assert [x.category for x in w] == [DeprecationWarning]


def test_from_stress_from_traction_aliases():  # PY-096
    assert aka.FromStress is aka.FromHigherDim
    assert aka.FromTraction is aka.FromSameDim
    assert isinstance(aka.FromStress, type) and isinstance(aka.FromTraction, type)
