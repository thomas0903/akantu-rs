# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (D-101 (1), PY-053) mpirun snippet: `Mesh.distribute()`
# from Python on the world communicator, run under `mpirun -np N` with the
# oracle's MPI-enabled module and with the port's MPI build in the same image
# (python/oracle/mpi/run.sh). Upstream sources: python/py_mesh.cc
# (`distribute`, `isDistributed`, `getNodes`, `getConnectivities`,
# `getLowerBounds` / `getUpperBounds`), python/py_group_manager.cc (groups),
# src/mesh/mesh.cc (`Mesh::distribute` / `distributeImpl`: Scotch on rank 0
# seeded with `RandomGenerator<Int>::seed()`), python/akantu/__init__.py
# (mpi4py imported when `has_mpi()`), examples/python/solid_mechanics_model/
# plate-hole/plate.py (the mesh read on rank 0 only).
#
# Every rank writes `out.r<rank>.txt` (`key = value`, floats as float.hex):
# per mesh, the local nodes (bitwise), the not-ghost and ghost
# connectivities of every element type, the element groups (elements per
# type and ghost type, their node groups) and the node groups, the global
# bounding box, and the global id of every local node, derived from its
# coordinates against the whole mesh read separately on the rank (upstream's
# Python API does not expose global ids; the meshes' node coordinates are
# distinct, so the derived ids are the root's node numbers). The port also
# writes its own global ids (`Mesh.getGlobalNodesIds()`, akantu-rs
# extension) to `port.r<rank>.txt`, which run.sh checks against the derived
# ones. Inputs (work dir): plate_hole.msh, cube_sync.msh. Run with a fixed
# AKA_SEED and SCOTCH_PTHREAD_NUMBER=1 (reproducible Scotch partitions).
#
# Two documented differences are left out of the compared output, on both
# modules alike: zero-size element-group blocks (UD-1062, C: upstream's
# root keeps an emptied block for every type a group had before the
# distribution, `eg.clear(type, _not_ghost)`; the port creates blocks only
# for types with elements, as the library's oracle comparisons do) and the
# `msh_block_<tag>` groups of unnamed physical tags (UD-045, B: upstream's
# MSH reader creates them empty, the port fills them by default; plate.msh
# has one, `msh_block_8`).
import sys

import numpy as np

import akantu as aka

from mpi4py import MPI  # noqa: E402 (both modules: already imported by akantu)

comm = MPI.COMM_WORLD
rank, size = comm.Get_rank(), comm.Get_size()
sys.stdout = open(f"out.r{rank}.txt", "w")
from _out import out  # noqa: E402

PORT = hasattr(aka.Mesh, "getGlobalNodesIds")
port_out = open(f"port.r{rank}.txt", "w") if PORT else None

out("has_mpi", bool(aka.has_mpi()))
out("size", size)

CASES = [("plate", 2, "plate_hole.msh"), ("cube", 3, "cube_sync.msh")]
GHOSTS = ((aka._not_ghost, "not_ghost"), (aka._ghost, "ghost"))


def node_key(x):
    return tuple(float(v).hex() for v in x)


def by_name(groups):
    return sorted(groups, key=lambda g: g.getName())


for name, dim, path in CASES:
    whole = aka.Mesh(dim, f"{name}_whole")
    whole.read(path)
    index = {node_key(x): i for i, x in enumerate(np.array(whole.getNodes()))}
    assert len(index) == whole.getNbNodes(), "duplicate node coordinates"

    mesh = aka.Mesh(dim, name)
    if rank == 0:
        mesh.read(path)
    out(f"{name}.before.is_distributed", bool(mesh.isDistributed()))
    out(f"{name}.before.nb_nodes", mesh.getNbNodes())
    mesh.distribute()
    out(f"{name}.is_distributed", bool(mesh.isDistributed()))
    nodes = np.array(mesh.getNodes())
    out(f"{name}.nb_nodes", mesh.getNbNodes())
    out(f"{name}.nodes", nodes)
    out(f"{name}.lower", np.array(mesh.getLowerBounds()))
    out(f"{name}.upper", np.array(mesh.getUpperBounds()))
    derived = [index[node_key(x)] for x in nodes]
    out(f"{name}.global_ids", derived)
    conns = mesh.getConnectivities()
    for gt, gname in GHOSTS:
        for t in conns.elementTypes(ghost_type=gt):
            out(f"{name}.conn.{gname}.{t}", np.array(conns(t, gt)))
            out(f"{name}.nb_element.{gname}.{t}",
                mesh.getNbElement(element_type=t, ghost_type=gt))
    for g in by_name(mesh.iterateElementGroups()):
        gn = g.getName()
        if gn.startswith("msh_block_"):
            continue  # UD-045
        els = g.getElements()
        for gt, gname in GHOSTS:
            for t in els.elementTypes(ghost_type=gt):
                block = np.array(els(t, gt))
                if block.shape[0] > 0:  # UD-1062
                    out(f"{name}.egroup.{gn}.{gname}.{t}", block)
        out(f"{name}.egroup.{gn}.nodes", np.array(g.getNodeGroup().getNodes()))
    for g in by_name(mesh.iterateNodeGroups()):
        if g.getName().startswith("msh_block_"):
            continue  # UD-045
        out(f"{name}.ngroup.{g.getName()}", np.array(g.getNodes()))
    if PORT:
        port_out.write(f"{name}.derived = {derived!r}\n")
        port_out.write(f"{name}.native = {list(mesh.getGlobalNodesIds())!r}\n")
        port_out.write(f"{name}.nb_global_nodes = {mesh.getNbGlobalNodes()} "
                       f"{whole.getNbNodes()}\n")
    del mesh, whole

sys.stdout.flush()
comm.Barrier()
