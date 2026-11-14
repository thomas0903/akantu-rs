# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case, pattern "writes from inside BC functors / callbacks":
# a Python functor writes into views of model arrays it holds (upstream:
# NumPy views alias the C++ arrays, so the writes land in the model).
#
# (a) Dirichlet functor that also zeroes/sets the velocity of the nodes it
#     constrains (explicit dynamics), then 5 explicit steps;
# (b) Neumann functor that also accumulates into a retained external-force
#     view;
# (c) Dirichlet functor that writes the displacement of *another* node
#     through a retained view.
# Expected (upstream reference semantics): every write is visible after
# applyBC and changes the trajectory. Each case prints the values and a
# CHECK against the upstream expectation.
import numpy as np

import akantu as aka
from cc1_common import check, edge_nodes, explicit_model, out, static_model

# (a) ----------------------------------------------------------------------
mesh, model = explicit_model()
model.setTimeStep(model.getStableTimeStep() * 0.5)
vel = model.getVelocity()  # retained view
nodes = edge_nodes(mesh)


class FixAndKick(aka.DirichletFunctor):
    def __call__(self, node, flags, primal, coord):
        primal[0] = 0.0
        flags[0] = True
        vel[node, 1] = 3.0  # write through a retained view, inside applyBC


model.applyBC(FixAndKick(), "edge")
seen = np.array(vel[nodes, 1], copy=True)
out("a_vel_after_applyBC", seen)
check("p2a_velocity_written_in_functor_is_kept", np.all(seen == 3.0),
      f"vel[edge,1]={seen.tolist()}")
for _ in range(5):
    model.solveStep()
out("a_disp_after_5_steps", model.getDisplacement())
uy = np.array(model.getDisplacement())[nodes, 1]
check("p2a_kicked_nodes_moved", np.all(uy > 0.0), f"u[edge,1]={uy.tolist()}")

# (b) ----------------------------------------------------------------------
mesh, model = static_model()
fext = model.getExternalForce()  # retained view
calls = []


class TractionAndTally(aka.NeumannFunctor):
    def __call__(self, quad_point, dual, coord, normals):
        dual[:] = [1.0, 0.0]
        calls.append(1)
        fext[0, 1] += 1.0  # count calls in node 0's y force


model.applyBC(TractionAndTally(), "edge")
f0y = float(fext[0, 1])
out("b_fext_node0_y", [f0y])
check("p2b_force_written_in_functor_is_kept", f0y == float(len(calls)),
      f"fext[0,1]={f0y} calls={len(calls)}")

# (c) ----------------------------------------------------------------------
mesh, model = static_model()
u = model.getDisplacement()
nodes = edge_nodes(mesh)
other = edge_nodes(mesh, "coh")[-1]  # a node outside "edge"


class FixAndPull(aka.DirichletFunctor):
    def __call__(self, node, flags, primal, coord):
        primal[:] = 0.0
        flags[:] = True
        u[other, 0] = 1e-6  # imposed on another node


model.applyBC(FixAndPull(), "edge")
v = float(u[other, 0])
out("c_u_other", [v])
check("p2c_displacement_written_in_functor_is_kept", v == 1e-6,
      f"u[other,0]={v}")
