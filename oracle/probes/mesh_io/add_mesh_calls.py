#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Lists the meshes the
# reference generates at build time with `add_mesh()` (cmake/Modules/
# FindGMSH.cmake): `gmsh -<DIM> -order <ORDER> -o <binary dir>/<OUTPUT> <geo>`,
# OUTPUT defaulting to `<geo name>.msh`, DIM to 3, ORDER to 1.
#
# Usage: add_mesh_calls.py <reference source root>   → JSON list on stdout
"""Static parser of the `add_mesh(...)` calls of test/, examples/ and
extra_packages/."""

import json
import os
import re
import sys


def calls(src: str) -> list[dict]:
    out = []
    for top in ("test", "examples", "extra_packages"):
        for d, _, files in os.walk(os.path.join(src, top)):
            if "CMakeLists.txt" not in files:
                continue
            cm = os.path.join(d, "CMakeLists.txt")
            text = "\n".join(l.split("#")[0] for l in open(cm, encoding="utf-8").read().splitlines())
            rel_dir = os.path.relpath(d, src)
            for m in re.finditer(r"\badd_mesh\s*\(([^)]*)\)", text, flags=re.IGNORECASE):
                args = m.group(1).split()
                if len(args) < 2:
                    raise SystemExit(f"{cm}: malformed add_mesh({m.group(1)})")
                target, geo, rest = args[0], args[1], args[2:]
                kw = {}
                unparsed = []
                i = 0
                while i < len(rest):
                    if rest[i] in ("OUTPUT", "DIM", "ORDER") and i + 1 < len(rest):
                        kw[rest[i]] = rest[i + 1]
                        i += 2
                    else:
                        unparsed.append(rest[i])
                        i += 1
                dim, order = kw.get("DIM"), kw.get("ORDER")
                if dim is None and order is None:
                    dim = unparsed[0] if len(unparsed) > 0 else None
                    order = unparsed[1] if len(unparsed) > 1 else None
                dim = dim or "3"
                order = order or "1"
                output = kw.get("OUTPUT") or os.path.splitext(os.path.basename(geo))[0] + ".msh"
                if any("$" in x for x in (geo, dim, order, output)):
                    raise SystemExit(f"{cm}: add_mesh with variables is not supported: {args}")
                out.append({
                    "target": target,
                    "cmake": os.path.relpath(cm, src),
                    "geo": os.path.normpath(os.path.join(rel_dir, geo)),
                    "dim": int(dim),
                    "order": int(order),
                    "output": os.path.normpath(os.path.join(rel_dir, output)),
                })
    out.sort(key=lambda c: (c["output"], c["geo"]))
    seen = {}
    for c in out:
        if c["output"] in seen and seen[c["output"]] != (c["geo"], c["dim"], c["order"]):
            raise SystemExit(f"two add_mesh calls write {c['output']} differently")
        seen[c["output"]] = (c["geo"], c["dim"], c["order"])
    # identical duplicates (same output, same command) are listed once
    uniq = {c["output"]: c for c in out}
    return [uniq[k] for k in sorted(uniq)]


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: add_mesh_calls.py <reference source root>")
    json.dump(calls(sys.argv[1]), sys.stdout, indent=1)
    sys.stdout.write("\n")
