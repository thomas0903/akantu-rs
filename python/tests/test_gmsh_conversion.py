# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (PY-095):
# `akantu.convertGmshToAkantuMesh(gmsh)` of python/akantu/__init__.py
# (upstream code; corrected by default,
# UD-1172 / UD-1173 category B, `upstream_compat=True` runs upstream's code)
# driven by the gmsh Python API. Skipped when the `gmsh` module is not
# importable (it is in neither the port's default environment nor the
# oracle image; the Python tier installs Debian's python3-gmsh,
# docs/TESTING.md).
"""convertGmshToAkantuMesh against the oracle module's result."""

import ast
import importlib.util

import numpy as np
import pytest

import akantu as aka
from _paths import REPO

gmsh = pytest.importorskip("gmsh")

SNIPPET = REPO / "python/oracle/snippets/gmsh_convert.py"
FIXTURE = REPO / "fixtures/python/gmsh_convert/oracle.txt"


def snippet():
    spec = importlib.util.spec_from_file_location("gmsh_convert", SNIPPET)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def conv(v):
    """The `_out.py` encoding of a value (floats as `float.hex`)."""
    if isinstance(v, np.ndarray):
        if v.dtype.kind == "f":
            return {"f64": [float(x).hex() for x in v.ravel()],
                    "shape": list(v.shape)}
        return {"dtype": str(v.dtype), "shape": list(v.shape),
                "values": v.ravel().tolist()}
    return v


@pytest.fixture
def session():
    gmsh.initialize()
    gmsh.option.setNumber("General.Terminal", 0)
    yield
    gmsh.finalize()


def oracle_lines(prefix):
    out = {}
    for line in FIXTURE.read_text().splitlines():
        if " = " in line and line.startswith(prefix):
            k, v = line.split(" = ", 1)
            try:
                out[k] = ast.literal_eval(v)
            except (ValueError, SyntaxError):
                out[k] = v
    return out


def test_single_type_model_matches_the_oracle_module(session):
    """A hand-built gmsh model with one element type (no meshing algorithm:
    platform independent) converts to the same Akantu mesh as with
    upstream's module (recorded by the oracle snippet `gmsh_convert.py` in
    the oracle image with Debian's python3-gmsh, bitwise)."""
    s = snippet()
    s.single_type_model(gmsh)
    items, error = s.attempt(aka, gmsh)
    assert error is None, error
    got = {f"single.{k}": conv(v) for k, v in items}
    want = oracle_lines("single.")
    assert want, "oracle fixture missing"
    assert got == want


def test_single_type_model_independent_checks(session):
    """Independent of the oracle: every Akantu element has the coordinates
    of its gmsh element's nodes, and the physical groups become element
    groups with their elements and nodes."""
    s = snippet()
    s.single_type_model(gmsh)
    tags, coords, _ = gmsh.model.mesh.get_nodes()
    xy = {int(t): coords[3 * i:3 * i + 2] for i, t in enumerate(tags)}
    mesh = aka.convertGmshToAkantuMesh(gmsh)
    nodes = mesh.getNodes()
    assert nodes.shape == (9, 2)
    _, gconn = gmsh.model.mesh.get_elements_by_type(s.QUAD4)
    conn = mesh.getConnectivity(aka._quadrangle_4)
    assert conn.shape == (4, 4)
    for e in range(4):
        for k in range(4):
            np.testing.assert_array_equal(nodes[conn[e, k]],
                                          xy[int(gconn[e * 4 + k])])
    groups = {g.getName(): g for g in mesh.iterateElementGroups()}
    assert set(groups) == {"left", "right"}
    q = aka._quadrangle_4
    assert groups["left"].getElements()(q).ravel().tolist() == [0, 1]
    assert groups["right"].getElements()(q).ravel().tolist() == [2, 3]
    assert len(groups["left"].getNodeGroup().getNodes()) == 6
    assert len(groups["right"].getNodeGroup().getNodes()) == 6


@pytest.mark.parametrize("case", ["pyramid", "tri10", "multi", "empty",
                                  "group_tag"])
def test_failures_are_upstream_s(session, case):
    """With the compat switch (`upstream_compat=True`): unsupported
    element types raise upstream's `ValueError` / `IndexError`; a model
    with several element types, or none, raises the `ValueError` of
    upstream's `if not types:` on a NumPy array (UD-1172, reproduced); a
    physical group whose tag is no entity tag raises gmsh's exception
    (UD-1173, reproduced): same exception type and message as the oracle
    module."""
    s = snippet()
    build = {
        "pyramid": lambda: s.single_type_model(gmsh, s.PYRAMID5),
        "tri10": lambda: s.single_type_model(gmsh, s.TRI10),
        "multi": lambda: s.multi_type_model(gmsh),
        "empty": lambda: gmsh.model.add("empty"),
        "group_tag": lambda: s.single_type_model(gmsh, all_group=True),
    }[case]
    build()
    items, error = s.attempt(aka, gmsh)
    assert items is None
    assert error == oracle_lines(f"{case}.error")[f"{case}.error"]


# --- the corrected default (UD-1172 / UD-1173, category B) -------------------


def default_convert(aka_, gmsh_):
    return aka_.convertGmshToAkantuMesh(gmsh_)


def gmsh_elements(dim=-1, tag=-1):
    """{gmsh element tag: (gmsh type, [node tags])} of the current model."""
    out = {}
    types, tags, nodes = gmsh.model.mesh.get_elements(dim, tag)
    for t, ts, ns in zip(types, tags, nodes):
        nn = len(ns) // max(len(ts), 1)
        for k, e in enumerate(ts):
            out[int(e)] = (int(t), [int(n) for n in ns[k * nn:(k + 1) * nn]])
    return out


def check_against_gmsh(mesh):
    """Independent of upstream: every gmsh element is an Akantu element of
    the mapped type with the same node coordinates (in gmsh's order), and
    every physical group is an element group holding exactly the elements
    of its entities (compared through their node coordinates)."""
    tags, coords, _ = gmsh.model.mesh.get_nodes()
    dim = mesh.getNodes().shape[1]
    xyz = {int(t): tuple(coords[3 * i:3 * i + dim]) for i, t in enumerate(tags)}
    nodes = mesh.getNodes()
    akatype = {1: aka._segment_2, 2: aka._triangle_3, 3: aka._quadrangle_4,
               15: aka._point_1}

    def key(t, conn_row):
        return (int(t), tuple(tuple(nodes[n]) for n in conn_row))

    def gkey(gt, ns):
        return (int(akatype[gt]), tuple(xyz[n] for n in ns))

    all_aka = []
    for t in mesh.getConnectivities().elementTypes():
        conn = mesh.getConnectivity(t)
        all_aka += [key(t, conn[e]) for e in range(conn.shape[0])]
    all_gmsh = [gkey(gt, ns) for gt, ns in gmsh_elements().values()]
    assert sorted(all_aka) == sorted(all_gmsh)
    groups = {g.getName(): g for g in mesh.iterateElementGroups()}
    want_names = set()
    for d, ptag in gmsh.model.get_physical_groups():
        name = gmsh.model.get_physical_name(d, ptag)
        want_names.add(name)
        want = []
        for ent in gmsh.model.get_entities_for_physical_group(d, ptag):
            want += [gkey(gt, ns) for gt, ns in gmsh_elements(d, ent).values()]
        els = groups[name].getElements()
        got = []
        for t in els.elementTypes():
            conn = mesh.getConnectivity(t)
            got += [key(t, conn[e]) for e in els(t).ravel()]
        assert sorted(got) == sorted(want), name
    assert set(groups) == want_names


def test_default_converts_a_multi_type_model(session):
    """UD-1172 corrected: four element types convert (upstream raises)."""
    s = snippet()
    s.multi_type_model(gmsh)
    mesh = aka.convertGmshToAkantuMesh(gmsh)
    assert mesh.getNodes().shape == (8, 2)
    assert {int(t) for t in mesh.getConnectivities().elementTypes()} == {
        int(aka._quadrangle_4), int(aka._triangle_3), int(aka._segment_2),
        int(aka._point_1)}
    check_against_gmsh(mesh)


def test_default_converts_a_group_whose_tag_is_no_entity_tag(session):
    """UD-1173 corrected: the group `all` (tag 3) of surfaces 1 and 2 holds
    the four quadrangles (upstream asks gmsh for surface 3)."""
    s = snippet()
    s.single_type_model(gmsh, all_group=True)
    mesh = aka.convertGmshToAkantuMesh(gmsh)
    groups = {g.getName(): g for g in mesh.iterateElementGroups()}
    q = aka._quadrangle_4
    assert groups["all"].getElements()(q).ravel().tolist() == [0, 1, 2, 3]
    assert len(groups["all"].getNodeGroup().getNodes()) == 9
    check_against_gmsh(mesh)


def test_default_converts_a_meshed_model(session):
    """A rectangle meshed by gmsh (points, lines, triangles): converts by
    default (upstream raises, UD-1172); checked against gmsh's own data
    (the mesh itself is platform dependent, so not against a fixture)."""
    gmsh.model.add("meshed")
    gmsh.model.occ.addRectangle(0.0, 0.0, 0.0, 2.0, 1.0)
    gmsh.model.occ.synchronize()
    gmsh.model.addPhysicalGroup(2, [1], name="plate")
    gmsh.model.addPhysicalGroup(1, [1, 2, 3, 4], name="boundary")
    gmsh.option.setNumber("Mesh.MeshSizeMax", 0.4)
    gmsh.model.mesh.generate(2)
    mesh = aka.convertGmshToAkantuMesh(gmsh)
    check_against_gmsh(mesh)


def test_default_equals_upstream_where_upstream_works(session):
    """On the single-type model (upstream converts it) the corrected
    default gives exactly the oracle module's result."""
    s = snippet()
    s.single_type_model(gmsh)
    items, error = s.attempt(aka, gmsh, default_convert)
    assert error is None, error
    got = {f"single.{k}": conv(v) for k, v in items}
    assert got == oracle_lines("single.")


@pytest.mark.parametrize("case", ["pyramid", "tri10", "empty"])
def test_default_failures(session, case):
    """The corrected default keeps upstream's errors for unsupported types,
    and an empty model raises the intended "Empty mesh" error."""
    s = snippet()
    {
        "pyramid": lambda: s.single_type_model(gmsh, s.PYRAMID5),
        "tri10": lambda: s.single_type_model(gmsh, s.TRI10),
        "empty": lambda: gmsh.model.add("empty"),
    }[case]()
    items, error = s.attempt(aka, gmsh, default_convert)
    assert items is None
    if case == "empty":
        assert error == "ValueError: Empty mesh: no element types found"
    else:
        assert error == oracle_lines(f"{case}.error")[f"{case}.error"]
