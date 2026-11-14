#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (no upstream counterpart): writes oracle/probes/parallel/pstrip.msh,
# the strip: a mesh made periodic in x on
# the root BEFORE it is distributed (parallel probe scenarios
# pbd_strip_scotch / pbd_strip_mesh_data and the Rust replays).
#
# 24 x 4 unit cells on [0, 24] x [0, 4], every cell split into two Tri3
# (v00 v10 v11) and (v00 v11 v01), cells row by row; Seg2 boundary groups
# bottom / top / left / right (left = x = 0: the periodic masters of
# makePeriodic(_x); right = x = 24: the slaves) and the surface group
# "strip". Integer coordinates (exact). Gmsh 2.2 ASCII, 125 nodes,
# 192 + 56 elements. Node (i, j) has 0-based id 25 j + i.
#
# The size is chosen so that upstream's distribution is memory-safe
# (UD-1060): with the seeded Scotch partition of the probe (seed 1234) no
# rank whose entry of synchronizePeriodicity's vector is relocated (rank 0
# at 2 ranks, ranks 0 and 1 at 4 ranks) holds a periodic slave (seed scan
# of the periodic-partition work: the 16 x 3 strip gives unsafe partitions for the
# seeds 1234..1293).
import sys

NX, NY = 24, 4  # overridden by the optional arguments NX NY (seed scans)


def nid(i, j):
    return j * (NX + 1) + i + 1  # 1-based


def main(out):
    lines = ["$MeshFormat", "2.2 0 8", "$EndMeshFormat", "$PhysicalNames", "5"]
    names = [(2, 1, "strip"), (1, 2, "bottom"), (1, 3, "top"), (1, 4, "left"),
             (1, 5, "right")]
    lines += [f'{d} {t} "{n}"' for d, t, n in names]
    lines += ["$EndPhysicalNames", "$Nodes", str((NX + 1) * (NY + 1))]
    for j in range(NY + 1):
        for i in range(NX + 1):
            lines.append(f"{nid(i, j)} {i} {j} 0")
    lines.append("$EndNodes")
    els = []  # (gmsh type, physical, elementary, nodes)
    for j in range(NY):
        for i in range(NX):
            v00, v10, v11, v01 = nid(i, j), nid(i + 1, j), nid(i + 1, j + 1), nid(i, j + 1)
            els.append((2, 1, 1, [v00, v10, v11]))
            els.append((2, 1, 1, [v00, v11, v01]))
    for i in range(NX):
        els.append((1, 2, 2, [nid(i, 0), nid(i + 1, 0)]))
    for i in range(NX):
        els.append((1, 3, 3, [nid(i + 1, NY), nid(i, NY)]))
    for j in range(NY):
        els.append((1, 4, 4, [nid(0, j + 1), nid(0, j)]))
    for j in range(NY):
        els.append((1, 5, 5, [nid(NX, j), nid(NX, j + 1)]))
    lines += ["$Elements", str(len(els))]
    for k, (t, p, g, ns) in enumerate(els, start=1):
        lines.append(f"{k} {t} 2 {p} {g} " + " ".join(map(str, ns)))
    lines.append("$EndElements")
    with open(out, "w", encoding="ascii") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    if len(sys.argv) > 3:
        NX, NY = int(sys.argv[2]), int(sys.argv[3])
    main(sys.argv[1] if len(sys.argv) > 1 else "pstrip.msh")
