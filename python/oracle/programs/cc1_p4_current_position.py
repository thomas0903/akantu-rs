# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case: in-place write into the displacement followed by a
# model call that reads the current position.
#
# Upstream `updateCurrentPosition()` recomputes `current_position = X + u`
# whenever `current_position_release != displacement_release`; predictor()
# and corrector() bump `displacement_release` in every solve, and nothing in
# a small-strain elastic solve (static Newton or explicit lumped) calls
# getCurrentPosition. So after `solveStep()` the cache is out of date and
# the next getCurrentPosition()/getStableTimeStep() recomputes it from the
# displacement *including the Python write*.
#
# The port's mirror sync-out (src/py/model.rs ModelHandle::sync_out) calls
# `model.current_position()` at the end of every model call while any model
# mirror exists, refreshing the cache at the current release; the later
# Python write does not bump the release, so the next call reuses the cache
# computed before the write.
import numpy as np

import akantu as aka
from cc1_common import check, edge_nodes, explicit_model, out, static_model

# (a) static: getCurrentPosition after write -------------------------------
mesh, model = static_model()
u = model.getDisplacement()  # any retained model view
model.getBlockedDOFs()[edge_nodes(mesh), :] = True  # clamped edge, no load
model.solveStep()  # u stays 0; bumps displacement_release (predictor/corrector)
u[:, 0] += 1e-4  # rigid translation written from Python
cp = np.array(model.getCurrentPosition(), copy=True)
X = np.array(mesh.getNodes(), copy=True)
expect = X + np.array(u)
out("a_current_position", cp)
check("p4a_current_position_includes_python_write",
      np.array_equal(cp, expect),
      f"max|cp-(X+u)|={np.abs(cp - expect).max():.3e}")

# (b) explicit: stable time step after write --------------------------------
mesh, model = explicit_model()
u = model.getDisplacement()
model.setTimeStep(model.getStableTimeStep() * 0.1)
model.solveStep()  # one explicit step (u stays 0: no load)
X = np.array(mesh.getNodes())
u[:, :] = -0.5 * X  # shrink the specimen to half its size (x and y)
dt = model.getStableTimeStep()
out("b_stable_time_step", [dt])
# independent reference: upstream computes dt on X + u = X - X/2 = X/2
# (exact, Sterbenz); a fresh model on a mesh whose nodes were set to X/2
# before the model existed computes dt on exactly that configuration.
aka.parseInput("elastic.dat")
mesh2 = aka.Mesh(2)
mesh2.read("mesh_dcb_2d.msh")
mesh2.getNodes()[:, :] = 0.5 * np.array(mesh2.getNodes())
model2 = aka.SolidMechanicsModel(mesh2)
model2.initFull(_analysis_method=aka._explicit_lumped_mass)
dt_ref = model2.getStableTimeStep()
out("b_stable_time_step_ref", [dt_ref])
check("p4b_stable_time_step_uses_written_displacement", dt == dt_ref,
      f"dt={dt!r} dt_ref={dt_ref!r} ratio={dt / dt_ref:.6f}")
