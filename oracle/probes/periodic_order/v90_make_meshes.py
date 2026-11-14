# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (UD-654): writes the structured MSH 2.2
# meshes and the case list of the adversarial periodic-order probe
# (v90_periodic_order_probe.cc and its Rust replay). Not ported from Akantu.
#
# usage: python3 -I v90_make_meshes.py <out-dir> [<frozen test_mesh_periodic dir>]
# (the second argument, e.g.
# fixtures/meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_periodic, adds
# the frozen line/square/cube meshes of upstream's periodic tests)
# Node numbering variants (the processing and insertion orders of makePeriodic
# follow the node ids): natural (x fastest), reversed, seeded shuffle.
import os
import random
import shutil
import sys


def grid(dim, n, order, seed):
    pts = []
    if dim == 1:
        pts = [(i / n, 0.0, 0.0) for i in range(n + 1)]
    elif dim == 2:
        pts = [(i / n, j / n, 0.0) for j in range(n + 1) for i in range(n + 1)]
    else:
        pts = [(i / n, j / n, k / n) for k in range(n + 1) for j in range(n + 1)
               for i in range(n + 1)]
    m = len(pts)
    perm = list(range(m))  # new id of natural node i
    if order == "rev":
        perm = list(reversed(perm))
    elif order == "shuf":
        rng = random.Random(seed)
        rng.shuffle(perm)
    def nid(i, j=0, k=0):
        return perm[i + (n + 1) * (j + (n + 1) * k)]
    elems = []
    if dim == 1:
        elems = [(1, [nid(i), nid(i + 1)]) for i in range(n)]
    elif dim == 2:
        elems = [(3, [nid(i, j), nid(i + 1, j), nid(i + 1, j + 1), nid(i, j + 1)])
                 for j in range(n) for i in range(n)]
    else:
        elems = [(5, [nid(i, j, k), nid(i + 1, j, k), nid(i + 1, j + 1, k), nid(i, j + 1, k),
                      nid(i, j, k + 1), nid(i + 1, j, k + 1), nid(i + 1, j + 1, k + 1),
                      nid(i, j + 1, k + 1)])
                 for k in range(n) for j in range(n) for i in range(n)]
    nodes = [None] * m
    for nat, p in enumerate(pts):
        nodes[perm[nat]] = p
    return nodes, elems


def write(path, nodes, elems):
    with open(path, "w") as f:
        f.write("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n%d\n" % len(nodes))
        for i, p in enumerate(nodes):
            f.write("%d %.17g %.17g %.17g\n" % (i + 1, p[0], p[1], p[2]))
        f.write("$EndNodes\n$Elements\n%d\n" % len(elems))
        for e, (t, conn) in enumerate(elems):
            f.write("%d %d 2 1 1 %s\n" % (e + 1, t, " ".join(str(c + 1) for c in conn)))
        f.write("$EndElements\n")


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    cases = []
    specs = [
        (1, 6, ["x"]),
        (2, 3, ["x", "y", "xy", "yx"]),
        (2, 10, ["xy", "yx"]),
        (2, 25, ["xy", "yx"]),
        (3, 2, ["xyz", "zyx", "yxz", "xy", "zx"]),
        (3, 5, ["xyz", "zyx", "xzy", "yz"]),
        (3, 9, ["xyz", "zyx"]),
    ]
    for dim, n, dirs_list in specs:
        for order in ["nat", "rev", "shuf"]:
            name = "g%dd_n%d_%s" % (dim, n, order)
            nodes, elems = grid(dim, n, order, seed=1000 * dim + n)
            write(os.path.join(out, name + ".msh"), nodes, elems)
            for dirs in dirs_list:
                cases.append("%s_%s %s.msh %d %s" % (name, dirs, name, dim, dirs))
    if len(sys.argv) > 2:
        frozen = [("line", 1, ["x"]), ("square", 2, ["xy", "yx"]),
                  ("cube", 3, ["xy", "xyz", "zyx", "yzx"])]
        for mname, dim, dirs_list in frozen:
            shutil.copy(os.path.join(sys.argv[2], mname + ".msh"),
                        os.path.join(out, "frozen_" + mname + ".msh"))
            for dirs in dirs_list:
                cases.append("frozen_%s_%s frozen_%s.msh %d %s" % (mname, dirs, mname, dim, dirs))
    with open(os.path.join(out, "cases.txt"), "w") as f:
        f.write("\n".join(cases) + "\n")


main()
