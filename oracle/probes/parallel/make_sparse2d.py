#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (no upstream counterpart): writes oracle/probes/parallel/sparse2d.msh,
# the distribution case of an audit (second half):
# sparse and unordered Gmsh node and element tags, several element types and
# several element groups (physical names), distributed by the parallel probe
# scenario distribution_sparse_mixed and replayed by the Rust test.
#
# 6 x 4 unit cells on [0, 6] x [0, 4]; cell (i, j) is a Quad4 when
# (i + 2 j) mod 3 == 0, otherwise two Tri3 (v00 v10 v11) and (v00 v11 v01)
# (types interleaved in the file). Surface groups matA (i < 2), matB
# (2 <= i < 4), matC (i >= 4); Seg2 groups bottom / top / left / right and
# an interior line `inner` along x = 3; Point1 groups `corners` (the four
# corners) and `probe` (node (3, 2), on the inner line).
# Nodes are listed in grid order (node (i, j) is the n-th, n = 7 j + i, so
# its 0-based index after reading is n) with the sparse, unordered tag
# 1000 + 37 k, k = (11 n) mod 35 (tags neither contiguous nor increasing in
# the file). Element tags: 500 + 13 m (m = position in the file), not
# contiguous.
# Gmsh 2.2 ASCII, 35 nodes.
import sys

NX, NY = 6, 4
NN = (NX + 1) * (NY + 1)  # 35


def k_of(i, j):
    return (11 * (j * (NX + 1) + i)) % NN  # 11 is invertible mod 35


def tag(i, j):
    return 1000 + 37 * k_of(i, j)


def main(out):
    assert len({k_of(i, j) for i in range(NX + 1) for j in range(NY + 1)}) == NN
    lines = ["$MeshFormat", "2.2 0 8", "$EndMeshFormat", "$PhysicalNames", "10"]
    names = [(2, 11, "matA"), (2, 12, "matB"), (2, 13, "matC"), (1, 21, "bottom"),
             (1, 22, "top"), (1, 23, "left"), (1, 24, "right"), (1, 25, "inner"),
             (0, 31, "corners"), (0, 32, "probe")]
    lines += [f'{d} {t} "{n}"' for d, t, n in names]
    lines += ["$EndPhysicalNames", "$Nodes", str(NN)]
    for j in range(NY + 1):
        for i in range(NX + 1):
            lines.append(f"{tag(i, j)} {i} {j} 0")
    lines.append("$EndNodes")
    els = []  # (gmsh type, physical, elementary, tags)
    for j in range(NY):
        for i in range(NX):
            v00, v10, v11, v01 = tag(i, j), tag(i + 1, j), tag(i + 1, j + 1), tag(i, j + 1)
            phys = 11 if i < 2 else 12 if i < 4 else 13
            if (i + 2 * j) % 3 == 0:
                els.append((3, phys, phys, [v00, v10, v11, v01]))
            else:
                els.append((2, phys, phys, [v00, v10, v11]))
                els.append((2, phys, phys, [v00, v11, v01]))
    for i in range(NX):
        els.append((1, 21, 21, [tag(i, 0), tag(i + 1, 0)]))
    for i in range(NX):
        els.append((1, 22, 22, [tag(i + 1, NY), tag(i, NY)]))
    for j in range(NY):
        els.append((1, 23, 23, [tag(0, j + 1), tag(0, j)]))
    for j in range(NY):
        els.append((1, 24, 24, [tag(NX, j), tag(NX, j + 1)]))
    for j in range(NY):
        els.append((1, 25, 25, [tag(3, j), tag(3, j + 1)]))
    for (i, j) in [(0, 0), (NX, 0), (NX, NY), (0, NY)]:
        els.append((15, 31, 31, [tag(i, j)]))
    els.append((15, 32, 32, [tag(3, 2)]))
    lines += ["$Elements", str(len(els))]
    for m, (t, p, g, ns) in enumerate(els):
        lines.append(f"{500 + 13 * m} {t} 2 {p} {g} " + " ".join(map(str, ns)))
    lines.append("$EndElements")
    with open(out, "w", encoding="ascii") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "sparse2d.msh")
