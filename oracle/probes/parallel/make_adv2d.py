#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (no upstream counterpart): writes oracle/probes/parallel/adv2d.msh,
# the adversarial mixed-element 2-D mesh, used by the
# parallel probe scenario `distribution_adv2d` and the Rust regressions of
# UD-231 (orphan lower-dimensional elements).
#
# 6 x 3 unit cells; cell (i, j) is a Quad4 when i + j is even, otherwise two
# Tri3 (v0 v1 v2) and (v0 v2 v3) (types interleaved in the file). Seg2 groups
# bottom/top/left/right and an interior `crack` along y = 1 crossing every
# partition interface; Point1 groups `pts` (on segments) and `orphan` (node
# (4, 2), on no segment: its element_to_subelement row is empty); surface
# groups matA (i < 3) and matB (i >= 3) straddling the parts. Gmsh 2.2 ASCII,
# 28 nodes, 55 elements. Node (i, j) has 0-based id 7 j + i.
import sys

NX, NY = 6, 3


def nid(i, j):
    return j * (NX + 1) + i + 1  # 1-based


def main(out):
    lines = ["$MeshFormat", "2.2 0 8", "$EndMeshFormat", "$PhysicalNames", "9"]
    names = [(2, 1, "matA"), (2, 2, "matB"), (1, 3, "bottom"), (1, 4, "top"),
             (1, 5, "left"), (1, 6, "right"), (1, 7, "crack"), (0, 8, "pts"),
             (0, 9, "orphan")]
    lines += [f'{d} {t} "{n}"' for d, t, n in names]
    lines += ["$EndPhysicalNames", "$Nodes", str((NX + 1) * (NY + 1))]
    for j in range(NY + 1):
        for i in range(NX + 1):
            lines.append(f"{nid(i, j)} {i} {j} 0")
    lines.append("$EndNodes")
    els = []  # (gmsh type, physical, elementary, nodes)
    for j in range(NY):
        for i in range(NX):
            v = [nid(i, j), nid(i + 1, j), nid(i + 1, j + 1), nid(i, j + 1)]
            phys = 1 if i < 3 else 2
            if (i + j) % 2 == 0:
                els.append((3, phys, phys, v))
            else:
                els.append((2, phys, phys, [v[0], v[1], v[2]]))
                els.append((2, phys, phys, [v[0], v[2], v[3]]))
    for i in range(NX):
        els.append((1, 3, 3, [nid(i, 0), nid(i + 1, 0)]))
    for i in range(NX):
        els.append((1, 4, 4, [nid(i, NY), nid(i + 1, NY)]))
    for j in range(NY):
        els.append((1, 5, 5, [nid(0, j), nid(0, j + 1)]))
    for j in range(NY):
        els.append((1, 6, 6, [nid(NX, j), nid(NX, j + 1)]))
    for i in range(NX):
        els.append((1, 7, 7, [nid(i, 1), nid(i + 1, 1)]))
    for (i, j) in [(0, 0), (3, 1), (6, 3)]:
        els.append((15, 8, 8, [nid(i, j)]))
    els.append((15, 9, 9, [nid(4, 2)]))
    lines += ["$Elements", str(len(els))]
    for k, (t, p, e, v) in enumerate(els, start=1):
        lines.append(f"{k} {t} 2 {p} {e} " + " ".join(map(str, v)))
    lines.append("$EndElements")
    with open(out, "w", encoding="ascii", newline="\n") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "adv2d.msh")
