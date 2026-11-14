# SPDX-License-Identifier: LGPL-3.0-or-later
# Adversarial programs of the Python bindings: shared helpers.
# Upstream API only, so every program also runs against the C++ oracle's
# `akantu` module. Inputs (copied into the working directory by
# python/oracle/replay-in-oracle-image.sh):
# mesh_dcb_2d.msh, elastic.dat (upstream test/test_python_interface).
import sys

import numpy as np

import akantu as aka

DIM = 2


def static_model():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(DIM)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    return mesh, model


def explicit_model():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(DIM)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    return mesh, model


def edge_nodes(mesh, group="edge"):
    return [int(n) for n in
            mesh.getElementGroup(group).getNodeGroup().getNodes().ravel()]


def out(key, value):
    """One result line: key, then float64 values as hex (bitwise)."""
    a = np.asarray(value, dtype=np.float64).ravel()
    print(key, a.size, " ".join(float(x).hex() for x in a))
    sys.stdout.flush()


def check(name, ok, detail=""):
    print(f"CHECK {name} {'PASS' if ok else 'FAIL'} {detail}")
    sys.stdout.flush()
