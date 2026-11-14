# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet:
# `SolverVector.isDistributed()` of the DOF manager's residual
# (python/py_solver.cc:93-94, python/py_dof_manager.cc:162-164). Upstream's
# `Mesh::distribute()` marks the mesh distributed on one process too
# (src/mesh/mesh.cc:551-552), and `DOFManagerDefault(mesh)` then allocates
# its residual as a `SolverVectorDistributed`
# (src/model/common/dof_manager/dof_manager_default.cc:52-73): `False` on a
# mesh that was not distributed, `True` after `distribute()`. Read after
# `initFull` (the port's manager binds to the mesh at its first nodal
# registration, parity row DOF-013). The sparse solver is pinned to Eigen.
import akantu as aka
from _out import out

aka.parseInput("elastic.dat")
for distribute in (False, True):
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    if distribute:
        mesh.distribute()
    key = "distributed" if distribute else "serial"
    out(f"{key}.mesh.isDistributed", bool(mesh.isDistributed()))
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(
        aka.ModelOptions(
            aka._static,
            aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen),
        )
    )
    res = model.getDOFManager().getResidual()
    out(f"{key}.residual.isDistributed", bool(res.isDistributed()))
