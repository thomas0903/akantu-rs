# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case: views outliving their model and mesh are stale-but-safe
# (ARCHITECTURE.md, "Shared storage and its aliasing rules"): readable, writes ignored, no crash after the owners are gone.
import gc

import numpy as np

import akantu as aka
from cc1_common import check, static_model

mesh, model = static_model()
u = model.getDisplacement()
x = mesh.getNodes()
s = model.getMaterial(0).getStress(aka._quadrangle_8)
u[:, 0] = 1.0
model.assembleInternalForces()
del model, mesh
gc.collect()
lease = u
while type(lease).__name__ != "_ArrayLease":
    lease = lease.base
ok = (lease.is_stale() and np.all(u[:, 0] == 1.0) and x.shape == (1409, 2)
      and np.isfinite(s).all())
u[:] = 5.0  # writes into the retired block: harmless
check("p10_views_outlive_owners_stale_but_safe", ok and np.all(u == 5.0),
      f"stale={lease.is_stale()}")
