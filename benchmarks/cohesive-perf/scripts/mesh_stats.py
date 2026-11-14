#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: cohesive scalability harness (benchmarks/cohesive-perf). Not a
# port of Akantu code; it inspects the Gmsh meshes used by the upstream
# performance test (gitlab.com/akantu/performance-testing-cohesive).
"""Statistics of a Gmsh MSH 4.1 ASCII mesh, as one JSON object on stdout.

    python3 -I mesh_stats.py <mesh.msh> [--label NAME] [--h H]

Reports the file SHA-256 and size, the MSH format, the node count, the element
count per Gmsh element type and per physical group, and the edge-length
statistics of the tetrahedra (min/mean/max of the 6 edges, used to relate the
`h` of `cube.geo` to the effective mesh size).
"""

import argparse
import hashlib
import json
import os
import sys

import numpy as np

GMSH_TYPES = {1: "line2", 2: "triangle3", 4: "tetrahedron4", 15: "point1"}
NODES_PER_TYPE = {1: 2, 2: 3, 4: 4, 15: 1}


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def parse(path):
    with open(path, "r", encoding="ascii") as f:
        lines = f.read().split("\n")
    pos = 0

    def section(name):
        nonlocal pos
        while lines[pos].strip() != name:
            pos += 1
        pos += 1

    section("$MeshFormat")
    fmt = lines[pos].split()
    if fmt[0] != "4.1" or fmt[1] != "0":
        sys.exit(f"{path}: only MSH 4.1 ASCII is supported (got {fmt})")

    physical_names = {}
    if "$PhysicalNames" in (l.strip() for l in lines[:200]):
        section("$PhysicalNames")
        n = int(lines[pos])
        for i in range(n):
            dim, tag, name = lines[pos + 1 + i].split(maxsplit=2)
            physical_names[(int(dim), int(tag))] = name.strip('"')

    # Entities: physical tags per (dim, entity tag)
    section("$Entities")
    counts = [int(x) for x in lines[pos].split()]
    pos += 1
    entity_phys = {}
    for dim, n in enumerate(counts):
        for _ in range(n):
            t = lines[pos].split()
            pos += 1
            tag = int(t[0])
            off = 4 if dim == 0 else 7
            nphys = int(t[off])
            entity_phys[(dim, tag)] = [int(x) for x in t[off + 1: off + 1 + nphys]]

    section("$Nodes")
    nb_blocks, nb_nodes, min_tag, max_tag = (int(x) for x in lines[pos].split())
    pos += 1
    tags = np.empty(nb_nodes, dtype=np.int64)
    coords = np.empty((nb_nodes, 3))
    k = 0
    for _ in range(nb_blocks):
        _, _, param, n = (int(x) for x in lines[pos].split())
        if param:
            sys.exit("parametric nodes are not supported")
        pos += 1
        tags[k:k + n] = np.array(lines[pos:pos + n], dtype=np.int64)
        pos += n
        coords[k:k + n] = np.array(" ".join(lines[pos:pos + n]).split(),
                                   dtype=float).reshape(n, 3)
        pos += n
        k += n
    index = np.full(max_tag + 1, -1, dtype=np.int64)
    index[tags] = np.arange(nb_nodes)

    section("$Elements")
    nb_blocks, nb_elements, _, _ = (int(x) for x in lines[pos].split())
    pos += 1
    by_type = {}
    by_group = {}
    tets = []
    for _ in range(nb_blocks):
        dim, etag, etype, n = (int(x) for x in lines[pos].split())
        pos += 1
        name = GMSH_TYPES.get(etype, f"gmsh_type_{etype}")
        by_type[name] = by_type.get(name, 0) + n
        for p in entity_phys.get((dim, etag), []):
            g = physical_names.get((dim, p), f"{dim}:{p}")
            by_group[g] = by_group.get(g, 0) + n
        if etype == 4:
            conn = np.array(" ".join(lines[pos:pos + n]).split(),
                            dtype=np.int64).reshape(n, 5)[:, 1:]
            tets.append(index[conn])
        pos += n
    return {
        "format": "MSH 4.1 ASCII",
        "nodes": int(nb_nodes),
        "elements_total": int(nb_elements),
        "elements_by_type": dict(sorted(by_type.items())),
        "elements_by_physical_group": dict(sorted(by_group.items())),
        "_coords": coords,
        "_tets": np.concatenate(tets) if tets else np.zeros((0, 4), dtype=np.int64),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mesh")
    ap.add_argument("--label")
    ap.add_argument("--h", type=float)
    a = ap.parse_args()
    info = parse(a.mesh)
    coords, tets = info.pop("_coords"), info.pop("_tets")
    out = {"label": a.label, "h": a.h, "file": os.path.basename(a.mesh),
           "bytes": os.path.getsize(a.mesh), "sha256": sha256_file(a.mesh)}
    out.update(info)
    if len(tets):
        pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
        lens = np.concatenate([np.linalg.norm(coords[tets[:, i]] - coords[tets[:, j]], axis=1)
                               for i, j in pairs])
        out["tet_edge_length"] = {"min": float(lens.min()), "mean": float(lens.mean()),
                                  "max": float(lens.max())}
        lo, hi = coords.min(axis=0), coords.max(axis=0)
        out["bounding_box"] = [[float(lo[i]), float(hi[i])] for i in range(3)]
    print(json.dumps(out, sort_keys=False))


if __name__ == "__main__":
    main()
