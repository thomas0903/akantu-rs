# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 6) oracle snippet: 100 steps of upstream
# examples/python/contact_mechanics_model/compression/compression.py (CouplerSolidContact,
# explicit, damping of the velocity view) without the dumps. The port runs
# with its upstream-compatibility switches on (port extension
# setUpstreamCompat, absent upstream): with the corrected defaults (UD-002
# force assembly, UD-371 normal orientation, ...) the run leaves upstream's
# from step 11, when several contact elements share a node.
import akantu as aka
from _out import out

aka.parseInput("compression.dat")
mesh = aka.Mesh(2)
mesh.read("compression.msh")
coupler = aka.CouplerSolidContact(mesh)
if hasattr(coupler, "setUpstreamCompat"):  # the port only
    coupler.setUpstreamCompat(True)
solid = coupler.getSolidMechanicsModel()
keep = aka.MeshDataMaterialSelectorString("physical_names", solid)
solid.setMaterialSelector(keep)
coupler.initFull(_analysis_method=aka._explicit_lumped_mass)
surface = aka.PhysicalSurfaceSelector(mesh)
coupler.getContactMechanicsModel().getContactDetector().setSurfaceSelector(surface)
solid.applyBC(aka.FixedValue(0.0, aka._x), "sides")
coupler.setTimeStep(solid.getStableTimeStep() * 0.1)
velocity = solid.getVelocity()
for s in range(100):
    solid.applyBC(aka.IncrementValue(-(1e-2 / 20000), aka._y), "loading")
    solid.applyBC(aka.IncrementValue((1e-2 / 20000), aka._y), "fixed")
    coupler.solveStep()
    if s % 10 == 0:
        velocity *= 0.9
out("coupler.u", coupler.getDisplacement())
out("coupler.fc", coupler.getContactForce())
out("coupler.gaps", coupler.getContactMechanicsModel().getGaps())
