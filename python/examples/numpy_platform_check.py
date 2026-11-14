# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: the values that the
# Python example scripts compute with NumPy ALONE, before any call into
# akantu, evaluated on this machine so that two platforms can be compared.
# Not ported from Akantu; it does not import akantu.
"""NumPy-alone values of the example scripts, for a platform comparison.

    numpy_platform_check.py pulse BAR.msh OUT.npy
        the initial pulse of dynamics.py (lines 71-82) and custom-material.py
        (lines 142-153): for every node of the Gmsh 4.1 mesh BAR.msh,
        ``A * np.sin(k * x) * np.exp(-(k * x) * (k * x) / (L * L))`` with
        ``x = position - 5.0``, evaluated exactly as the scripts do (a loop
        over NumPy float64 scalars), nodes sorted by (x, y), saved as .npy
    numpy_platform_check.py norm ARRAY.npy
        ``np.linalg.norm`` of an array (the residual norms of
        phasefield-static.py, lines 97-98), printed as a hex float
    numpy_platform_check.py compare A.npy B.npy
        elementwise comparison: number of differing elements, the largest
        absolute and relative difference and the largest difference in ulps
"""

import sys

import numpy as np


def nodes(path):
    lines = open(path).read().split("\n")
    i = lines.index("$Nodes")
    n_blocks, _n_nodes, _lo, _hi = (int(x) for x in lines[i + 1].split())
    pos = []
    i += 2
    for _ in range(n_blocks):
        _dim, _tag, _par, count = (int(x) for x in lines[i].split())
        i += 1 + count  # entity header, then the node tags
        for j in range(count):
            pos.append([float(x) for x in lines[i + j].split()])
        i += count
    pos = np.array(pos)
    return pos[np.lexsort((pos[:, 1], pos[:, 0]))]


def pulse(position):
    pulse_width = 1
    A = 0.01
    out = np.zeros(len(position))
    for i in range(len(position)):
        x = position[i, 0] - 5.0
        L = pulse_width
        k = 0.1 * 2 * np.pi * 3 / L
        out[i] = A * np.sin(k * x) * np.exp(-(k * x) * (k * x) / (L * L))
    return out


def main(argv):
    if len(argv) == 4 and argv[1] == "pulse":
        np.save(argv[3], pulse(nodes(argv[2])))
        print("numpy", np.__version__, "pulse of", argv[2])
    elif len(argv) == 3 and argv[1] == "norm":
        print("numpy", np.__version__, "norm", float(np.linalg.norm(np.load(argv[2]))).hex())
    elif len(argv) == 4 and argv[1] == "compare":
        a, b = np.load(argv[2]), np.load(argv[3])
        d = a - b
        ulp = np.abs(a.view(np.int64) - b.view(np.int64))
        nz = d != 0
        rel = np.abs(d[nz]) / np.maximum(np.abs(a[nz]), np.abs(b[nz])) if nz.any() else np.zeros(1)
        print(f"{int(nz.sum())} of {a.size} elements differ; largest absolute difference "
              f"{np.abs(d).max():.3e}, largest relative {rel.max():.3e}, largest {int(ulp.max())} ulp")
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
