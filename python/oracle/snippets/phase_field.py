# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 5) oracle snippet: the staggered flow of upstream
# examples/python/phase_field_model/phasefield-static.py (CouplerSolidPhaseField,
# savePreviousState, coupler.solve with named solvers, views of the
# displacement / blocked DOFs / damage) on the four-element mesh and the
# no-split AT2 input of test_phase_solid_coupling, with the sparse solver
# pinned to Eigen through initFull(ModelOptions) (the oracle's default is
# MUMPS) and a fixed number of staggered iterations per step. The port runs with its
# upstream-compatibility switches on (port extension setUpstreamCompat,
# absent upstream).

import akantu as aka
from _out import out

aka.parseInput("material_coupling.dat")
mesh = aka.Mesh(2)
mesh.read("test_four_elements.msh")
coupler = aka.CouplerSolidPhaseField(mesh)
if hasattr(coupler, "setUpstreamCompat"):  # the port only
    coupler.setUpstreamCompat(True)
solid = coupler.getSolidMechanicsModel()
phase = coupler.getPhaseFieldModel()
eigen = aka.ModelOptions(
    aka._static, aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen)
)
solid.initFull(eigen)
phase.initFull(eigen)
for model, threshold in ((solid, 1e-8), (phase, 1e-4)):
    solver = model.getNonLinearSolver("static")
    solver.set("max_iterations", 100)
    solver.set("threshold", threshold)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)

X = mesh.getNodes()
bottom = X[:, 1] < X[:, 1].min() + 1e-12
top = X[:, 1] > X[:, 1].max() - 1e-12
left = X[:, 0] < X[:, 0].min() + 1e-12
blocked = solid.getBlockedDOFs()
blocked[bottom, 1] = True
blocked[top, 1] = True
blocked[left, 0] = True
u = solid.getDisplacement()
damage = phase.getDamage()
for step in range(12):
    phase.savePreviousState()
    u[top, 1] += 2e-3
    for _ in range(3):
        coupler.solve("static", "static")
    out(f"pf.{step:02d}.u", u)
    out(f"pf.{step:02d}.damage", damage)
out("pf.epot", solid.getEnergy("potential"))
out("pf.dissipated", phase.getEnergy("dissipated"))
for t in solid.getMaterial(0).getInternalReal("stress").elementTypes():
    out("pf.stress", solid.getMaterial(0).getInternalReal("stress")(t))
    out("pf.strain", phase.getPhaseField(0).getInternalReal("strain")(t))
