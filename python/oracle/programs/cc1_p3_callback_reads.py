# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case: reads from inside a BC functor through a retained
# view of an array that the same native call is modifying. Upstream: the
# view aliases the array, so node k's functor sees the values written for
# nodes visited before it. The functor below imposes, on each node of the
# group, primal[0] = (value seen at the previously visited node) + 1e-6, a
# legitimate "ramp along the boundary" construction.
import numpy as np

import akantu as aka
from cc1_common import check, out, static_model

mesh, model = static_model()
u = model.getDisplacement()
order = []


class Ramp(aka.DirichletFunctor):
    def __call__(self, node, flags, primal, coord):
        prev = order[-1] if order else None
        base = float(u[prev, 0]) if prev is not None else 0.0
        primal[0] = base + 1e-6
        flags[0] = True
        order.append(int(node))


model.applyBC(Ramp(), "edge")
vals = np.array([u[n, 0] for n in order])
out("ramp_order", order)
out("ramp_values", vals)
expected = np.array([(i + 1) * 1e-6 for i in range(len(order))])
# upstream: base = value just written for the previous node -> cumulative
# sum 1e-6, 2e-6, ... (exact sums of the same additions in visit order)
acc, exp = 0.0, []
for _ in order:
    acc = acc + 1e-6
    exp.append(acc)
check("p3_functor_sees_values_written_earlier_in_the_same_call",
      np.array_equal(vals, np.array(exp)), f"values={vals.tolist()}")
