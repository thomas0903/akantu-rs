# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 3) oracle snippet: python/py_dumpable.cc on a mesh
# and an element group; prints the written files and their contents' hash.
import hashlib
import os

import akantu as aka
from _out import out

mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
mesh.registerDumperParaview("mesh", "mesh_dump", True)
mesh.addDumpMesh(mesh, 2)
mesh.dump()
mesh.dump(3)
group = mesh.getElementGroup("edge")
group.dump()
for name in ("addDumpField",):
    try:
        getattr(mesh, name)("positions")
        out(f"{name}", "accepted")
    except Exception as e:  # noqa: BLE001
        out(f"{name}", type(e).__name__)
found = []
for root, _, names in os.walk("paraview"):
    for n in names:
        found.append(os.path.join(root, n))
for f in sorted(found):
    data = open(f, "rb").read()
    out(f"file:{f}", hashlib.sha256(data).hexdigest())
