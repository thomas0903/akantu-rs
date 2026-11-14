# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case, pattern 1-3: in-place writes into model arrays from
# Python followed by solves (temporary view, retained view `u += …`, views
# retained across steps, views dropped before the call, writes through
# NumPy derived views), checked bitwise against the same problem imposed by
# the native built-in functor FixedValue (no Python write involved).
import gc

import numpy as np

import akantu as aka
from cc1_common import check, edge_nodes, out, static_model

D1, D2 = 0.5e-5, 1e-5


def native(steps):
    mesh, model = static_model()
    model.applyBC(aka.FixedValue(0.0, aka._x), "edge")
    model.applyBC(aka.FixedValue(0.0, aka._y), "edge")
    res = []
    for d in steps:
        model.applyBC(aka.FixedValue(d, aka._x), "coh")
        model.solveStep()
        res.append(np.array(model.getDisplacement(), copy=True))
    return res


def temp_views(steps):
    mesh, model = static_model()
    e, c = edge_nodes(mesh, "edge"), edge_nodes(mesh, "coh")
    model.getBlockedDOFs()[e, :] = True
    model.getBlockedDOFs()[c, 0] = True
    res = []
    for d in steps:
        model.getDisplacement()[c, 0] = d
        model.solveStep()
        res.append(np.array(model.getDisplacement(), copy=True))
    return res


def retained_views(steps):
    mesh, model = static_model()
    e, c = edge_nodes(mesh, "edge"), edge_nodes(mesh, "coh")
    u = model.getDisplacement()
    b = model.getBlockedDOFs()
    b[e, :] = True
    b[c, 0] = True
    res, held = [], []
    prev = 0.0
    for d in steps:
        u[c, 0] += d - prev  # `u += …` on a retained view
        prev = d
        model.solveStep()
        held.append(np.array(u, copy=True))  # no refetch
        res.append(np.array(model.getDisplacement(), copy=True))
    return res, held


def dropped_and_derived(steps):
    mesh, model = static_model()
    e, c = edge_nodes(mesh, "edge"), edge_nodes(mesh, "coh")
    b = model.getBlockedDOFs()
    flat_b = b.reshape(-1)  # derived view
    for n in e:
        flat_b[2 * n] = True
        flat_b[2 * n + 1] = True
    for n in c:
        flat_b[2 * n] = True
    del b, flat_b
    gc.collect()
    res = []
    for d in steps:
        u = model.getDisplacement()
        col = u[:, 0]  # derived (strided) view
        vals = np.array(col, copy=True)
        vals[c] = d
        np.copyto(col, vals)
        del u, col
        gc.collect()  # all views dropped before the call
        model.solveStep()
        res.append(np.array(model.getDisplacement(), copy=True))
    return res


steps = [D1, D2]
ref = native(steps)
for i, r in enumerate(ref):
    out(f"native_step{i}", r)
variants = {
    "temp": temp_views(steps),
    "dropped_derived": dropped_and_derived(steps),
}
ret, held = retained_views(steps)
variants["retained"] = ret
variants["retained_held"] = held
for name, res in variants.items():
    for i, r in enumerate(res):
        same = np.array_equal(r.view(np.uint64), ref[i].view(np.uint64))
        check(f"p1_{name}_step{i}_bitwise_vs_native", same,
              f"max|diff|={np.abs(r - ref[i]).max():.3e}")
