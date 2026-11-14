#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (EX-008, UD-851): the first-order version of
# the frozen second-order mesh of upstream's examples/c++/embedded_model
# (concrete.geo, `add_mesh(concrete_mesh concrete.geo 2 2)`): every
# `_triangle_6` keeps its three corner nodes (`_triangle_3`), every
# `_segment_3` its two end nodes (`_segment_2`), the `_point_1` element
# stays; nodes that only mid-side entries used are dropped and the kept ones
# renumbered 1..N in their original order (coordinates written as Python's
# shortest round-trip repr, i.e. the same binary64 values); physical names
# and the physical / elementary tags of every element are kept. Gmsh builds
# a second-order mesh by adding mid-side nodes to the first-order one, and
# every edge of concrete.geo is straight, so this is the mesh
# `add_mesh(concrete_mesh concrete.geo 2 1)` gives on the same platform, up
# to node numbering, without depending on a Gmsh run (Gmsh meshes are
# platform-dependent, D-009). Output: Gmsh MSH 2.2 ASCII.
#
#   python3 -I derive_first_order.py <frozen concrete.msh (MSH 4.1)> <out.msh>
import sys

# Gmsh element type -> (first-order type, nodes kept)
FIRST_ORDER = {15: (15, 1), 1: (1, 2), 8: (1, 2), 2: (2, 3), 9: (2, 3)}


def sections(lines):
    out, name, body = {}, None, []
    for line in lines:
        if line.startswith("$End"):
            out[name] = body
            name, body = None, []
        elif line.startswith("$"):
            name = line[1:]
        elif name is not None:
            body.append(line)
    return out


def main(src, dst):
    with open(src, encoding="ascii") as fh:
        s = sections(fh.read().split("\n"))
    if s["MeshFormat"][0].split()[0] != "4.1":
        sys.exit("expected MSH 4.1")
    names = s["PhysicalNames"][1:]
    # physical tag of every entity (dim, tag); at most one per entity here
    ent = iter(s["Entities"])
    counts = [int(x) for x in next(ent).split()]
    phys = {}
    for dim, n in enumerate(counts):
        for _ in range(n):
            w = next(ent).split()
            tag = int(w[0])
            k = 4 if dim == 0 else 7  # tag + xyz, or tag + bounding box
            nphys = int(w[k])
            tags = [int(x) for x in w[k + 1 : k + 1 + nphys]]
            if len(tags) > 1:
                sys.exit(f"entity {dim} {tag}: several physical tags")
            phys[(dim, tag)] = tags[0] if tags else 0
    # nodes: blocks "dim tag parametric n", then n tags, then n coordinates
    nodes = {}
    it = iter(s["Nodes"][1:])
    for header in it:
        if not header.strip():
            continue
        n = int(header.split()[3])
        tags = [int(next(it)) for _ in range(n)]
        for t in tags:
            nodes[t] = [float(x) for x in next(it).split()]
    # elements: blocks "dim tag type n", then n lines "id nodes..."
    elements = []
    it = iter(s["Elements"][1:])
    for header in it:
        if not header.strip():
            continue
        dim, tag, ty, n = (int(x) for x in header.split())
        new_ty, keep = FIRST_ORDER[ty]
        for _ in range(n):
            w = [int(x) for x in next(it).split()]
            elements.append((new_ty, phys[(dim, tag)], tag, w[1 : 1 + keep]))
    used = sorted({v for e in elements for v in e[3]})
    renum = {old: new for new, old in enumerate(used, start=1)}
    out = ["$MeshFormat", "2.2 0 8", "$EndMeshFormat", "$PhysicalNames"]
    out += [str(len(names))] + names + ["$EndPhysicalNames", "$Nodes", str(len(used))]
    out += [f"{renum[t]} " + " ".join(repr(c) for c in nodes[t]) for t in used]
    out += ["$EndNodes", "$Elements", str(len(elements))]
    for i, (ty, ph, tag, conn) in enumerate(elements, start=1):
        out.append(f"{i} {ty} 2 {ph} {tag} " + " ".join(str(renum[v]) for v in conn))
    out += ["$EndElements", ""]
    with open(dst, "w", encoding="ascii", newline="\n") as fh:
        fh.write("\n".join(out))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__ if __doc__ else "usage: derive_first_order.py SRC DST")
    main(sys.argv[1], sys.argv[2])
