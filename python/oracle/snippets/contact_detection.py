# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 6) oracle snippet: a stand-alone
# ContactMechanicsModel search (upstream examples/python/contact_mechanics_model/compression/
# detection-explicit.py without the dumps) and the GeometryUtils helpers on
# its contact segments (PY-088, mesh node positions; not contravariantBasis
# nor covariantBasis with a normal that requests the 2-D flip, undefined
# upstream: UD-366, the oracle aborts).
import numpy as np

import akantu as aka
from _out import out

aka.parseInput("detection-explicit.dat")
cmesh = aka.Mesh(2)
cmesh.read("detection-explicit.msh")
contact = aka.ContactMechanicsModel(cmesh)
contact.initFull(_analysis_method=aka._explicit_lumped_mass)
contact.getContactDetector().setSurfaceSelector(aka.PhysicalSurfaceSelector(cmesh))
contact.search()
out("contact.gaps", contact.getGaps())
out("contact.normals", contact.getNormals())
out("contact.nb_elements", len(list(contact.getContactElements())))

X = cmesh.getNodes()
xi = np.array([0.3])
for i in (0, 3, 7):
    el = aka.Element(aka._segment_2, i, aka._not_ghost)
    n = aka.GeometryUtils.normal(cmesh, X, el)
    out(f"geo.{i}.normal", n)
    raw = aka.GeometryUtils.normal(cmesh, X, el, False)
    out(f"geo.{i}.normal_raw", raw)
    # the normal e_z x t (= -raw): no 2-D flip requested (UD-366)
    cov = aka.GeometryUtils.covariantBasis(cmesh, X, el, -raw, xi)
    out(f"geo.{i}.covariant", cov)
    out(f"geo.{i}.curvature", aka.GeometryUtils.curvature(cmesh, X, el, xi))
    slave = X[i + 1] + np.array([0.01, 0.02])
    out(f"geo.{i}.projection", aka.GeometryUtils.realProjection(cmesh, X, slave, el, n))
