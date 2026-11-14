# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: compare the outputs of an oracle snippet run with the
# upstream module (oracle image) and with the port.
"""Usage: compare.py ORACLE_OUT PORT_OUT [REL_TOL]

Each output line is ``key = <literal>`` (see snippets/_out.py). Strings,
integers and shapes must be equal; float arrays (``float.hex``) are compared
value by value: the report gives the number of bitwise-equal values and the
largest relative difference (normalised by the array's max norm). Exit code
1 if a key is missing, a non-float value differs, or a relative difference
exceeds REL_TOL (default 1e-12)."""

import ast
import sys


def load(path):
    d = {}
    with open(path) as f:
        for line in f:
            if " = " not in line:
                continue
            k, v = line.rstrip("\n").split(" = ", 1)
            try:
                d[k] = ast.literal_eval(v)
            except (ValueError, SyntaxError):
                d[k] = v
    return d


def floats(v):
    return [float.fromhex(x) for x in v["f64"]]


def cmp(a, b, stats):
    if isinstance(a, dict) and "f64" in a:
        if not (isinstance(b, dict) and "f64" in b) or a["shape"] != b["shape"]:
            return False
        fa, fb = floats(a), floats(b)
        norm = max([abs(x) for x in fa] + [0.0])
        for x, y in zip(fa, fb):
            stats["n"] += 1
            if x == y or (x != x and y != y):
                stats["bitwise"] += 1
            else:
                r = abs(x - y) / (norm if norm > 0 else 1.0)
                stats["max_rel"] = max(stats["max_rel"], r)
        return True
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(cmp(x, y, stats) for x, y in zip(a, b))
    return a == b


def main():
    oracle, port = load(sys.argv[1]), load(sys.argv[2])
    tol = float(sys.argv[3]) if len(sys.argv) > 3 else 1e-12
    ok = True
    for k in oracle:
        if k not in port:
            print(f"MISSING  {k}")
            ok = False
            continue
        stats = {"n": 0, "bitwise": 0, "max_rel": 0.0}
        same = cmp(oracle[k], port[k], stats)
        if not same:
            print(f"DIFFER   {k}: oracle={str(oracle[k])[:120]} port={str(port[k])[:120]}")
            ok = False
        elif stats["n"]:
            flag = "OK " if stats["max_rel"] <= tol else "TOL"
            ok &= stats["max_rel"] <= tol
            print(f"{flag}      {k}: {stats['bitwise']}/{stats['n']} bitwise, max rel diff {stats['max_rel']:.3g}")
        else:
            print(f"OK       {k}")
    for k in port:
        if k not in oracle:
            print(f"EXTRA    {k}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
