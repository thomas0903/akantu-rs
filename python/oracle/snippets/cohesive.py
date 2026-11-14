# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 4) oracle snippet: upstream
# examples/python/solid_mechanics_cohesive_model/cohesive/plate.py with the
# explicit solver only (no linear solve: the oracle's auto sparse solver is
# MUMPS and initFull of the cohesive model takes no solver options), 60
# steps; inserter, facet arrays, cohesive law internals, fragments.
import numpy as np

import akantu as aka
from _out import out

aka.parseInput("cohesive_material.dat")
mesh = aka.Mesh(2)
mesh.read("cohesive_plate.msh")
model = aka.SolidMechanicsModelCohesive(mesh)
model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
model.applyBC(aka.FixedValue(0.0, aka._x), "XBlocked")
model.applyBC(aka.FixedValue(0.0, aka._y), "YBlocked")
trac = np.zeros(2)
trac[int(aka._y)] = 0.095
model.getExternalForce()[:] = 0
model.applyBC(aka.FromTraction(trac), "Traction")
out("fext", model.getExternalForce())
inserter = model.getElementInserter()
out("check_facets", inserter.getCheckFacets(aka._segment_2))
out("insertion_facets", inserter.getInsertionFacets()(aka._segment_2))
mat = model.getMaterial(1)
out("mat.class", type(mat).__name__)
out("bulk.class", type(model.getMaterial(0)).__name__)
model.setTimeStep(model.getStableTimeStep() * 0.1)
total = 0
for i in range(60):
    n = model.checkCohesiveStress()
    total += n
    model.solveStep("explicit_lumped")
    if i % 10 == 0:
        out(f"step{i}.inserted", n)
        out(f"step{i}.epot", model.getEnergy("potential"))
        out(f"step{i}.ekin", model.getEnergy("kinetic"))
out("total_inserted", total)
out("u", model.getDisplacement())
out("tangents", model.getTangents(aka._segment_2))
out("stress_on_facets", model.getStressOnFacets(aka._segment_2))
out("facet_filter", mat.getFacetFilter(aka._segment_2))
if total:
    out("opening", mat.getOpening(aka._cohesive_2d_4))
    out("traction", mat.getTraction(aka._cohesive_2d_4))
fm = aka.FragmentManager(model)
fm.computeAllData()
out("fragments", fm.getNbFragment())
out("fragment.mass", fm.getMass())
out("fragment.velocity", fm.getVelocity())
out("fragment.inertia", fm.getMomentsOfInertia())
out("fragment.nb_elements", fm.getNbElementsPerFragment())
