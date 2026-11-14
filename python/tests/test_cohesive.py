# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (group 4): python/py_solid_mechanics_model_cohesive.cc
# (CohesiveElementInserter, SolidMechanicsModelCohesive(Options)),
# python/py_material.cc (MaterialCohesive and the cohesive law classes),
# python/py_constitutive_law_selector.cc (cohesive selectors),
# python/py_fragment_manager.cc; flows of upstream
# examples/python/solid_mechanics_cohesive_model/{cohesive,fragmentation,
# multi-material}.
"""Cohesive model, inserter, laws, selectors, fragments."""

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES

EX = "examples/python/solid_mechanics_cohesive_model"


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "plate.msh": MESHES / EX / "cohesive/plate.msh",
            "material.dat": PARSER_FIXTURES / EX / "cohesive/material.dat",
            "multi.msh": MESHES / EX / "multi-material/plate.msh",
            "multi.dat": PARSER_FIXTURES / EX / "multi-material/material.dat",
            "frag.msh": MESHES / EX / "fragmentation/fragmentation_mesh.msh",
            "frag.dat": PARSER_FIXTURES / EX / "fragmentation/material.dat",
        }
    )


def plate_model(extrinsic=True, method=aka._explicit_lumped_mass):
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    model.initFull(_analysis_method=method, _is_extrinsic=extrinsic)
    model.applyBC(aka.FixedValue(0.0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0.0, aka._y), "YBlocked")
    trac = np.zeros(2)
    trac[int(aka._y)] = 0.095
    model.getExternalForce()[:] = 0
    model.applyBC(aka.FromTraction(trac), "Traction")
    return mesh, model


def test_hierarchy():
    assert issubclass(aka.SolidMechanicsModelCohesive, aka.SolidMechanicsModel)
    assert issubclass(aka.MaterialCohesive, aka.Material)
    assert issubclass(aka.MaterialCohesiveLinear2D, aka.MaterialCohesive)
    assert issubclass(aka.MaterialCohesiveLinearFriction3D,
                      aka.MaterialCohesive)
    assert issubclass(aka.SolidMechanicsModelCohesiveOptions, aka.ModelOptions)
    assert issubclass(aka.FragmentManager, aka.GroupManager)
    for s in (aka.DefaultMaterialCohesiveSelector,
              aka.MeshDataMaterialCohesiveSelector,
              aka.MaterialCohesiveRulesSelector):
        assert issubclass(s, aka.ConstitutiveLawSelector)
    opts = aka.SolidMechanicsModelCohesiveOptions(aka._static,
                                                  is_extrinsic=True)
    assert isinstance(opts, aka.ModelOptions)
    # Python subclasses are runtime cohesive laws (D-078
    # section 1); the constructor wants a model, as Material's
    class Mine(aka.MaterialCohesive):
        pass
    with pytest.raises(TypeError, match="model must be a model object"):
        Mine(None, "x")


def test_plate_extrinsic_explicit():
    mesh, model = plate_model()
    mat = model.getMaterial(1)
    assert type(mat) is aka.MaterialCohesiveLinear2D
    assert type(model.getMaterial(0)) is aka.MaterialLinearElastic2D
    inserter = model.getElementInserter()
    assert isinstance(inserter, aka.CohesiveElementInserter)
    check = inserter.getCheckFacets()
    assert isinstance(check, aka.ElementTypeMapArraybool)
    facets = check(aka._segment_2)
    assert facets.dtype == np.bool_ and facets.any()
    assert not facets.flags.writeable
    assert inserter.getCheckFacets(aka._segment_2).shape == facets.shape
    ins = inserter.getInsertionFacets(aka._segment_2)
    assert ins.shape == facets.shape
    model.setTimeStep(model.getStableTimeStep() * 0.1)
    inserted = 0
    for _ in range(30):
        inserted += model.checkCohesiveStress()
        model.solveStep("explicit_lumped")
    stress = model.getStressOnFacets(aka._segment_2)
    assert stress.shape[1] == 8 and not stress.flags.writeable
    tangents = model.getTangents(aka._segment_2)
    assert tangents.shape[1] == 2
    ff = mat.getFacetFilter()
    assert isinstance(ff, aka.ElementTypeMapArrayInt)
    assert np.isfinite(model.getDisplacement()).all()
    if inserted:
        op = mat.getOpening(aka._cohesive_2d_4)
        tr = mat.getTraction(aka._cohesive_2d_4)
        assert op.shape == tr.shape and op.shape[1] == 2
    model.updateAutomaticInsertion()
    model.assembleInternalForces()


def test_inserter_settings():
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    inserter = model.getElementInserter()
    inserter.setLimit(aka._y, -0.5, 0.5)
    inserter.addPhysicalSurface("Traction")
    inserter.addPhysicalVolume("bulk")
    model.initFull(_analysis_method=aka._explicit_lumped_mass,
                   _is_extrinsic=True)
    with pytest.raises(aka.Exception):
        intrinsic = aka.SolidMechanicsModelCohesive(mesh)
        intrinsic.checkCohesiveStress()


def test_rules_selector_multi_material():
    aka.parseInput("multi.dat")
    mesh = aka.Mesh(2)
    mesh.read("multi.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    selector = aka.MaterialCohesiveRulesSelector(
        model,
        {
            ("Top", "Bottom"): "tough",
            ("Top", "Top"): "soft",
            ("Bottom", "Bottom"): "soft",
        },
    )
    model.setMaterialSelector(selector)
    assert model.getMaterialSelector() is selector
    model.initFull(_analysis_method=aka._static, _is_extrinsic=True)
    model.initNewSolver(aka._explicit_lumped_mass)
    names = [model.getMaterial(i).getName()
             for i in range(model.getNbMaterials())]
    assert names == ["Bottom", "soft", "tough"]


def test_fragment_manager():
    aka.parseInput("frag.dat")
    mesh = aka.Mesh(2)
    mesh.read("frag.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass,
                   _is_extrinsic=True)
    model.updateAutomaticInsertion()
    fm = aka.FragmentManager(model)
    fm.computeAllData()
    n = fm.getNbFragment()
    assert n == 1
    assert fm.getMass().shape == (n, 2)
    assert fm.getVelocity().shape == (n, 2)
    assert fm.getMomentsOfInertia().shape == (n, 2)
    assert fm.getPrincipalDirections().shape == (n, 4)
    assert fm.getNbElementsPerFragment().shape == (n, 1)
    fm.buildFragments(damage_limit=1.0)
    fm.computeCenterOfMass()
    fm.computeVelocity()
    fm.computeInertiaMoments()
    fm.computeNbElementsPerFragment()
    assert fm.getNbElementsPerFragment()[0, 0] == mesh.getNbElement(
        element_type=aka._triangle_3)


def _lease(view):
    """The `_ArrayLease` base object of an akantu view."""
    b = view
    while type(b).__name__ != "_ArrayLease":
        b = b.base
    return b


def test_int008_views_across_insertion_and_after_the_parents_deletion():
    """INT-008: live NumPy views of the cohesive model
    across a cohesive insertion and after the model and mesh are deleted
    (contract of PY-005 / 006, D-047 / D-056: a resize retires the viewed
    block — the old view keeps the values it had and writes to it reach
    nothing — and views outlive their owners stale but safe)."""
    import gc

    mesh, model = plate_model()
    mat = model.getMaterial(1)
    u = model.getDisplacement()
    x = mesh.getNodes()
    nb_nodes = x.shape[0]
    x0 = x.copy()
    model.setTimeStep(model.getStableTimeStep() * 0.1)
    inserted = 0
    for _ in range(400):
        inserted += model.checkCohesiveStress()
        model.solveStep("explicit_lumped")
        if inserted:
            break
    assert inserted > 0, "no cohesive element inserted"
    # the insertion duplicated nodes: the nodal blocks were resized
    x_new = mesh.getNodes()
    u_new = model.getDisplacement()
    assert x_new.shape[0] > nb_nodes and u_new.shape[0] == x_new.shape[0]
    assert _lease(x).is_stale() and _lease(u).is_stale()
    assert not _lease(x_new).is_stale() and not _lease(u_new).is_stale()
    assert x.shape == (nb_nodes, 2) and np.array_equal(x, x0)
    assert np.array_equal(x_new[:nb_nodes], x0)  # old nodes kept in place
    # writes to a retired view reach nothing
    before = u_new.copy()
    u[:] = 1.0e3
    assert np.array_equal(model.getDisplacement(), before)
    # the new views are live: a write is seen by the model
    u_new[0, 0] = 0.25
    assert model.getDisplacement()[0, 0] == 0.25
    u_new[0, 0] = before[0, 0]
    # cohesive connectivity and law internals of the inserted elements
    conn = mesh.getConnectivity(aka._cohesive_2d_4)
    opening = mat.getOpening(aka._cohesive_2d_4)
    assert conn.shape[0] == inserted and opening.shape[1] == 2
    nq = opening.shape[0] // conn.shape[0]
    assert nq * conn.shape[0] == opening.shape[0]
    assert conn.max() < x_new.shape[0]
    # more steps: further insertions retire these blocks too
    more = 0
    for _ in range(400):
        more += model.checkCohesiveStress()
        model.solveStep("explicit_lumped")
        if more:
            break
    assert more > 0, "no second cohesive insertion"
    assert _lease(conn).is_stale() and _lease(opening).is_stale()
    assert conn.shape[0] == inserted  # values at retirement
    assert mesh.getConnectivity(aka._cohesive_2d_4).shape[0] == inserted + more
    # the parent's deletion: every view stays readable and writable, stale
    held = (model.getDisplacement(), mesh.getNodes(),
            mesh.getConnectivity(aka._cohesive_2d_4),
            mat.getOpening(aka._cohesive_2d_4))
    snapshot = [np.array(h, copy=True) for h in held]
    del model, mesh, mat
    gc.collect()
    # the topology views of a mesh a model uses (connectivities, group
    # lists) are read-only (UD-532); the nodal views and the law internals
    # are not
    assert [h.flags.writeable for h in held] == [True, True, False, True]
    for h, s in zip(held, snapshot):
        assert _lease(h).is_stale()
        assert np.array_equal(h, s)
        if h.flags.writeable:
            h[...] = 0  # into the retired block: harmless
            assert not h.any()
    for v in (u, x, conn, opening):
        assert _lease(v).is_stale()


def test_upstream_compat_switch():
    # D-078 section 6: UD-034 and the cohesive switches
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    assert model.getUpstreamCompat() is False
    model.setUpstreamCompat(True)
    assert model.getUpstreamCompat() is True
    model.setUpstreamCompat(False)
    assert model.getUpstreamCompat() is False
    model.setUpstreamCompat()
    model.initFull(_analysis_method=aka._static, _is_extrinsic=True)
    assert model.getUpstreamCompat() is True
    with pytest.raises(aka.Exception, match="before init_full"):
        model.setUpstreamCompat(False)  # the DOF families exist


def test_init_constitutive_laws_is_the_cohesive_models():
    # InitConstitutiveLaws() (an akantu-rs
    # helper; upstream's SolidMechanicsModelCohesive::initConstitutiveLaws
    # is not bound) runs the cohesive model's law initialisation: the facet
    # laws and the insertion, never the bulk laws alone
    ct = aka._cohesive_2d_4

    def fresh():
        aka.parseInput("material.dat")
        mesh = aka.Mesh(2)
        mesh.read("plate.msh")
        return mesh, aka.SolidMechanicsModelCohesive(mesh)

    # intrinsic, upstream's zero-strength insertion (UD-224 compat): the
    # cohesive elements are inserted; the helper inserts nothing more
    mesh, model = fresh()
    model.setUpstreamCompat(True)
    model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=False)
    n = mesh.getNbElement(element_type=ct)
    assert n > 0
    model.initConstitutiveLaws()
    assert mesh.getNbElement(element_type=ct) == n
    # intrinsic, corrected default: the insertion into a law without
    # strength is refused (UD-224), by initFull and by the helper alike
    mesh, model = fresh()
    with pytest.raises(aka.Exception, match="outside the extrinsic insertion check"):
        model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=False)
    with pytest.raises(aka.Exception, match="outside the extrinsic insertion check"):
        model.initConstitutiveLaws()
    assert mesh.getNbElement(element_type=ct) == 0
    # extrinsic: the facet laws' filters are assigned; a second facet
    # assignment through the helper is refused (the facet internals exist)
    mesh, model = fresh()
    model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
    law = model.getMaterial(1)
    assert law.getFacetFilter()(aka._segment_2).shape[0] > 0
    with pytest.raises(aka.Exception, match="addFacet"):
        model.initConstitutiveLaws()


SPRING_DAT = """model solid_mechanics_model_cohesive [
  cohesive_inserter [
    bounding_box = [[0,10],[-10, 10]]
  ]
  material elastic [
     name = virtual
     rho = 1
     E   = 1
     nu  = 0.3
  ]
  material py2_spring [
     name = spring
     sigma_c = 0.1
     k = 2.5
  ]
]
"""


class Spring(aka.MaterialCohesive):
    """A runtime cohesive law: traction = k * opening, inserted on the
    first checkable facet of its filter at the first check."""

    def __init__(self, model, _id):
        super().__init__(model, _id)
        super().registerParamReal(
            "k", aka._pat_readable | aka._pat_parsable, "stiffness")
        self.registerInternalReal("delta_max", 1)
        self.calls = {"init": 0, "check": 0, "traction": 0}
        self.flagged = None

    def initMaterial(self):
        super().initMaterial()
        self.calls["init"] += 1
        self.sigma_c = self.getReal("sigma_c")
        self.k = self.getReal("k")

    def checkInsertion(self, check_only):
        self.calls["check"] += 1
        model = self.getModel()
        inserter = model.getElementInserter()
        facets = self.getFacetFilter()
        fe = model.getFEEngine("FacetsFEEngine")
        for t in facets.elementTypes(dim=1):
            f = facets(t)
            if f.shape[0] == 0:
                continue
            assert fe.getNbIntegrationPoints(t) >= 1
            assert model.getStressOnFacets(t).shape[0] > 0
            check = inserter.getCheckFacets(t)
            ins = inserter.getInsertionFacets(t)
            assert ins.flags.writeable
            if self.calls["check"] == 1:
                for facet in f[:, 0]:
                    if check[facet]:
                        ins[facet] = True
                        self.flagged = int(facet)
                        break

    def computeTraction(self, el_type, ghost_type):
        self.calls["traction"] += 1
        op = self.getOpening(el_type, ghost_type)
        tr = self.getTraction(el_type, ghost_type)
        tr[:, :] = self.k * op
        dm = self.getInternalReal("delta_max")(el_type, ghost_type)
        dm[:, 0] = np.maximum(dm[:, 0], np.linalg.norm(op, axis=1))


_LAWS = []


def _spring_allocator(_dim, _option, model, _id):
    law = Spring(model, _id)
    _LAWS.append(law)
    return law


def test_runtime_cohesive_law_python_subclass(tmp_path):
    # D-078 section 1: a Python MaterialCohesive subclass is a
    # runtime cohesive law: its parameters (k from the input file, sigma_c
    # of the base), its internal, initMaterial, checkInsertion (with the
    # model's accessors and writable insertion flags) and computeTraction
    if "py2_spring" not in aka.MaterialFactory.getInstance().getPossibleAllocators():
        aka.MaterialFactory.getInstance().registerAllocator(
            "py2_spring", _spring_allocator)
    (tmp_path / "spring.dat").write_text(SPRING_DAT)
    aka.parseInput(str(tmp_path / "spring.dat"))
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
    law = _LAWS[-1]
    assert law.calls["init"] == 1
    assert law.sigma_c == 0.1 and law.k == 2.5
    assert model.getMaterial(1) is law
    inserted = model.checkCohesiveStress()
    assert law.calls["check"] == 1 and law.flagged is not None
    assert inserted == 1
    # outside the checks the flags are read-only copies again
    ins = model.getElementInserter().getInsertionFacets(aka._segment_2)
    assert not ins.flags.writeable
    model.setTimeStep(model.getStableTimeStep() * 0.1)
    model.getVelocity()[:, 1] = 0.01 * model.getMesh().getNodes()[:, 1]
    model.solveStep()
    assert law.calls["traction"] >= 1
    op = law.getOpening(aka._cohesive_2d_4)
    tr = law.getTraction(aka._cohesive_2d_4)
    assert op.shape == tr.shape and op.shape[0] >= 1
    np.testing.assert_array_equal(tr, 2.5 * op)
    dm = law.getInternalReal("delta_max")(aka._cohesive_2d_4)
    assert dm.shape == (op.shape[0], 1)
    np.testing.assert_array_equal(dm[:, 0], np.linalg.norm(op, axis=1))
    # a second check inserts nothing (the law flags only at its first)
    assert model.checkCohesiveStress() == 0


def test_runtime_cohesive_law_exception_is_carried():
    # an exception of a Python cohesive law reaches the caller unchanged
    class Broken(Spring):
        def computeTraction(self, el_type, ghost_type):
            raise ZeroDivisionError("from computeTraction")

    def alloc(_dim, _option, model, _id):
        return Broken(model, _id)

    if "py2_broken" not in aka.MaterialFactory.getInstance().getPossibleAllocators():
        aka.MaterialFactory.getInstance().registerAllocator("py2_broken", alloc)
    import tempfile
    import os
    d = tempfile.mkdtemp()
    with open(os.path.join(d, "b.dat"), "w") as fh:
        fh.write(SPRING_DAT.replace("py2_spring", "py2_broken"))
    aka.parseInput(os.path.join(d, "b.dat"))
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
    assert model.checkCohesiveStress() == 1
    model.setTimeStep(model.getStableTimeStep() * 0.1)
    with pytest.raises(ZeroDivisionError, match="from computeTraction"):
        model.solveStep()


def _model_with_law(tmp_path, name, cls):
    """An extrinsic plate model whose cohesive law is `cls` (registered as
    `name`), from SPRING_DAT."""
    factory = aka.MaterialFactory.getInstance()
    if name not in factory.getPossibleAllocators():
        factory.registerAllocator(name, lambda _d, _o, model, _id: cls(model, _id))
    (tmp_path / f"{name}.dat").write_text(SPRING_DAT.replace("py2_spring", name))
    aka.parseInput(str(tmp_path / f"{name}.dat"))
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
    return mesh, model


def test_runtime_cohesive_law_reentrant_check_is_refused(tmp_path):
    # UD-985, port-only: a checkCohesiveStress() from
    # inside a Python checkInsertion is refused with a recoverable error;
    # the outer check keeps its flags and inserts them (upstream's nested
    # call would insert them itself), and later checks work. The other
    # mesh-changing calls of the binding, initFull and initConstitutiveLaws,
    # are refused there too.
    seen = {"calls": 0, "inner": None, "init": None, "laws": None, "flagged": []}

    class Reentrant(Spring):
        def checkInsertion(self, check_only):
            seen["calls"] += 1
            if seen["calls"] != 1:
                return
            model = self.getModel()
            inserter = model.getElementInserter()
            facets = self.getFacetFilter()
            for t in facets.elementTypes(dim=1):
                f = facets(t)
                check = inserter.getCheckFacets(t)
                ins = inserter.getInsertionFacets(t)
                for facet in f[:, 0]:
                    if len(seen["flagged"]) == 2:
                        break
                    if check[facet]:
                        ins[facet] = True
                        seen["flagged"].append(int(facet))
            with pytest.raises(aka.Exception, match="inside a checkInsertion") as e:
                model.checkCohesiveStress()
            seen["inner"] = str(e.value)
            with pytest.raises(aka.Exception, match="inside a checkInsertion"):
                model.initFull(
                    _analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
            seen["init"] = True
            with pytest.raises(aka.Exception, match="inside a checkInsertion"):
                model.initConstitutiveLaws()
            seen["laws"] = True
            # the outer check's flags are intact and still writable
            for t in facets.elementTypes(dim=1):
                ins = inserter.getInsertionFacets(t)
                assert ins.flags.writeable
                assert [int(i) for i in np.flatnonzero(np.asarray(ins))] == sorted(seen["flagged"])

    mesh, model = _model_with_law(tmp_path, "py2_reentrant", Reentrant)
    assert model.checkCohesiveStress() == 2
    assert len(seen["flagged"]) == 2 and seen["inner"] and seen["init"] and seen["laws"]
    assert mesh.getNbElement(element_type=aka._cohesive_2d_4) == 2
    # the inserter kept its flag map: a later check runs (and inserts
    # nothing: the law flags only at its first check)
    assert model.checkCohesiveStress() == 0
    assert seen["calls"] == 2
    assert mesh.getNbElement(element_type=aka._cohesive_2d_4) == 2


def test_runtime_cohesive_law_tangent_in_a_static_solve_after_insertion(tmp_path):
    # UD-986: a Python computeTangentTraction (upstream
    # py_material.cc:121-126, a reference to the tangent array) in a static
    # solve after an extrinsic insertion. Upstream's strict build aborts
    # that solve on a bounds assertion (aka_array_tmpl.hh:413, an array
    # sized before the insertion's new nodes); the port converges, and the
    # assembled tangent is the law's
    seen = {"checked": False, "tangent": 0, "shape": None}

    class SpringT(Spring):
        def checkInsertion(self, check_only):
            if seen["checked"]:
                return
            seen["checked"] = True
            model = self.getModel()
            inserter = model.getElementInserter()
            facets = self.getFacetFilter()
            n = 0
            for t in facets.elementTypes(dim=1):
                check = inserter.getCheckFacets(t)
                ins = inserter.getInsertionFacets(t)
                for facet in facets(t)[:, 0]:
                    if n < 3 and check[facet]:
                        ins[facet] = True
                        n += 1

        def computeTangentTraction(self, el_type, tangent_matrix, ghost_type):
            seen["tangent"] += 1
            seen["shape"] = tuple(tangent_matrix.shape)
            t = tangent_matrix.reshape(-1, 4)
            t[:, :] = self.k * np.array([1.0, 0.0, 0.0, 1.0])

    factory = aka.MaterialFactory.getInstance()
    if "py2_spring_t" not in factory.getPossibleAllocators():
        factory.registerAllocator("py2_spring_t", lambda _d, _o, model, _id: SpringT(model, _id))
    dat = SPRING_DAT.replace("py2_spring", "py2_spring_t").replace(
        "  material elastic [",
        "  time_step_solver static [\n    sparse_solver_type = eigen\n  ]\n"
        "  material elastic [", 1)
    (tmp_path / "spring_t.dat").write_text(dat)
    aka.parseInput(str(tmp_path / "spring_t.dat"))
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    model.initFull(_analysis_method=aka._static, _is_extrinsic=True)
    model.applyBC(aka.FixedValue(0.0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0.0, aka._y), "YBlocked")
    trac = np.zeros(2)
    trac[1] = 0.095
    model.getExternalForce()[:] = 0
    model.applyBC(aka.FromTraction(trac), "Traction")
    solver = model.getNonLinearSolver("static")
    solver.set("max_iterations", 100)
    solver.set("threshold", 1e-10)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.residual)
    model.solveStep("static")
    assert model.checkCohesiveStress() == 3
    model.solveStep("static")  # upstream's strict build aborts here
    assert seen["tangent"] >= 1 and seen["shape"][-1] == 4
    assert mesh.getNbElement(element_type=aka._cohesive_2d_4) == 3
    assert np.all(np.isfinite(model.getDisplacement()))
    law = model.getMaterial(1)
    op = law.getOpening(aka._cohesive_2d_4)
    np.testing.assert_array_equal(law.getTraction(aka._cohesive_2d_4), 2.5 * op)
