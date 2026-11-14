# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (group 3): python/py_dumpable.cc (Dumpable on Mesh
# and ElementGroup: registerDumperParaview, addDumpMesh(ToDumper),
# addDumpField(ToDumper), addDumpFieldExternal(ToDumper), dump overloads).
"""Dumpable: mesh and element-group dumpers."""

import os

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "mesh_dcb_2d.msh": MESHES
            / "test/test_python_interface/mesh_dcb_2d.msh",
        }
    )


def files(d="paraview"):
    out = []
    for root, _, names in os.walk(d):
        out += [os.path.join(root, n) for n in names]
    return sorted(out)


def test_hierarchy():
    assert issubclass(aka.ElementGroup, aka.Dumpable)
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    for name in ("registerDumperParaview", "addDumpMesh", "addDumpMeshToDumper",
                 "addDumpField", "addDumpFieldToDumper",
                 "addDumpFieldExternal", "addDumpFieldExternalToDumper",
                 "dump"):
        assert hasattr(mesh, name) and hasattr(mesh.getElementGroup("edge"),
                                               name)


def test_mesh_dumper_with_external_field():
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    mesh.registerDumperParaview("mesh", "mesh_dump", True)
    mesh.addDumpMesh(mesh, 2)
    ext = np.zeros((mesh.getNbNodes(), 2))
    mesh.addDumpFieldExternal("ext", ext)
    ext[:, 0] = mesh.getNodes()[:, 0]  # read at dump time
    mesh.dump()
    mesh.dump(1)
    mesh.dump(0.5, 2)
    mesh.dump("mesh")
    mesh.dump(dumper_name="mesh", step=4)
    mesh.dump("mesh", 1.0, 5)
    dumped = files()
    assert any("mesh_dump" in f for f in dumped), dumped
    vtu = [f for f in dumped if f.endswith(".vtu")]
    assert vtu
    text = open(vtu[0], "rb").read()
    assert b"ext" in text
    with pytest.raises(aka.Exception, match="not implemented"):
        mesh.addDumpField("positions")
    with pytest.raises(aka.Exception, match="not implemented"):
        mesh.addDumpFieldToDumper("mesh", "positions")
    with pytest.raises(TypeError):
        mesh.addDumpFieldExternalToDumper("mesh", "f", object())


def test_mesh_dumper_to_named_dumper():
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    mesh.registerDumperParaview("first", "first_file")
    mesh.registerDumperParaview(dumper_name="second", file_name="second_file",
                                is_default=False)
    mesh.addDumpMeshToDumper("second", mesh, 1)
    mesh.addDumpMeshToDumper(dumper_name="first", mesh=mesh, dimension=2,
                             ghost_type=aka._not_ghost,
                             element_kind=aka._ek_regular)
    mesh.dump("second")
    mesh.dump("first")
    dumped = files()
    assert any("second_file" in f for f in dumped)
    assert any("first_file" in f for f in dumped)
    other = aka.Mesh(2)
    with pytest.raises(aka.Exception, match="own mesh"):
        mesh.addDumpMesh(other, 2)


def test_element_group_default_dumper():
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    group = mesh.getElementGroup("edge")
    assert isinstance(group, aka.Dumpable)
    group.dump()
    assert any("edge" in f for f in files())
    group.registerDumperParaview("mine", "edge_mine", True)
    group.addDumpMesh(mesh, 1)
    group.dump()
    assert any("edge_mine" in f for f in files())
