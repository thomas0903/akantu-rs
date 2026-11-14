# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case "reverse stale": a Rust-side change made by a binding
# call that bypasses the mirror protocol is later overwritten by the stale
# mirror.
#
# Upstream semantics: Material.setReal(name, v) -> ParameterRegistry::set ->
# updateInternalParameters() (src/io/parser/parameter_registry.cc:137-152),
# and Material::updateInternalParameters() (src/model/solid_mechanics/
# material.cc:78-88) copies the `eigen_grad_u` *parameter* (zero by default)
# into every quadrature point of the `eigen_grad_u` internal. So an
# eigenstrain written through a view is discarded by any later setReal.
#
# Port: `Material.setReal` goes through MaterialRegistry::with_registry
# (src/py/model.rs), which borrows the model directly (no mirror sync-in /
# sync-out). The internal is reset in Rust, but the view's mirror still holds
# the written eigenstrain; the next model call's sync-in sees "mirror !=
# Rust" and writes the eigenstrain back.
import numpy as np

import akantu as aka
from cc1_common import check, edge_nodes, out, static_model

mesh, model = static_model()
e = edge_nodes(mesh)
model.getBlockedDOFs()[e, :] = True  # clamp the x = 0 edge, no load
mat = model.getMaterial(0)
eg = mat.getInternalReal("eigen_grad_u")(aka._quadrangle_8)
eg[:, 0] = 1e-3  # eigenstrain eps_xx everywhere (written through a view)
mat.setReal("E", mat.getReal("E"))  # upstream: resets eigen_grad_u to 0
model.solveStep()
u = np.array(model.getDisplacement())
eg_after = np.array(mat.getInternalReal("eigen_grad_u")(aka._quadrangle_8))
out("eigen_grad_u_after_solve_col0", eg_after[:, 0])
out("max_abs_u", [np.abs(u).max()])
check("p8_setReal_discards_eigenstrain_like_upstream",
      np.abs(u).max() == 0.0 and np.all(eg_after == 0.0),
      f"max|u|={np.abs(u).max():.3e} eigen_grad_u[0]={eg_after[0].tolist()}")
