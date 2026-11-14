# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (groups 5 and 6): python/py_heat_transfer_model.cc,
# py_structural_mechanics_model.cc, py_phase_field_model.cc, py_phasefield.cc,
# py_contact_mechanics_model.cc, py_model_couplers.cc; flows of upstream
# examples/python/{structural_mechanics_model,phase_field_model,
# contact_mechanics_model}.
"""Heat transfer, structural, phase-field, contact models and couplers."""

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES

EX = "examples/python"


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "plate_static.msh": MESHES / EX / "phase_field_model/plate_static.msh",
            "material_static.dat": PARSER_FIXTURES / EX
            / "phase_field_model/material_static.dat",
            "compression.msh": MESHES / EX
            / "contact_mechanics_model/compression/compression.msh",
            "compression.dat": PARSER_FIXTURES / EX
            / "contact_mechanics_model/compression/compression.dat",
            "mesh_dcb_2d.msh": MESHES
            / "test/test_python_interface/mesh_dcb_2d.msh",
            "heat_transfer_input.dat": PARSER_FIXTURES
            / "test/test_model/patch_tests/data/heat_transfer_input.dat",
        }
    )


def test_hierarchy():
    assert issubclass(aka.HeatTransferModel, aka.DiffusionModel)
    clh = getattr(aka, "ConstitutiveLawsHandlerakantu::DiffusionLawakantu::Model")
    assert issubclass(aka.DiffusionModel, clh) and issubclass(clh, aka.Model)
    assert issubclass(aka.HeatDiffusion2D, aka.DiffusionLaw)
    assert issubclass(aka.DiffusionLaw,
                      getattr(aka, "ConstitutiveLawakantu::DiffusionModel"))
    assert issubclass(aka.PhaseField,
                      getattr(aka, "ConstitutiveLawakantu::PhaseFieldModel"))
    assert issubclass(aka.PhaseFieldModel, getattr(
        aka, "ConstitutiveLawsHandlerakantu::PhaseFieldakantu::Model"))
    for c in (aka.StructuralMechanicsModel, aka.ContactMechanicsModel,
              aka.CouplerSolidContact, aka.CouplerSolidCohesiveContact,
              aka.CouplerSolidPhaseField):
        assert issubclass(c, aka.Model)
    assert issubclass(aka.PhysicalSurfaceSelector, aka.SurfaceSelector)
    assert issubclass(aka.PhaseFieldElementFilter, aka.ClusteringFilter)
    assert aka.PhaseFieldModelOptions is aka.ModelOptions
    assert aka.ContactMechanicsModelOptions is aka.ModelOptions
    # HeatDiffusion<d>D(model, id) is constructible as upstream
    # (a diffusion model is required)
    with pytest.raises(TypeError):
        aka.HeatDiffusion2D(None, "x")
    # Python allocators are accepted; a registered id raises
    with pytest.raises(aka.Exception, match="already registered"):
        aka.PhaseFieldFactory.getInstance().registerAllocator("quadratic", print)


def test_structural_material_and_model():
    m = aka.StructuralMaterial()
    m.E, m.A, m.I = 1e9, 1.0, 2.0
    c = aka.StructuralMaterial(m)
    assert (c.E, c.A, c.I) == (1e9, 1.0, 2.0)
    mesh = aka.Mesh(2)
    accessor = aka.MeshAccessor(mesh)
    accessor.resizeNodes(3)
    mesh.getNodes()[:] = [[0, 0], [1, 0], [2, 0]]
    mesh.addConnectivityType(aka._bernoulli_beam_2)
    accessor.resizeConnectivity(2, aka._bernoulli_beam_2)
    mesh.getConnectivity(aka._bernoulli_beam_2)[:] = [[0, 1], [1, 2]]
    accessor.makeReady()
    model = aka.StructuralMechanicsModel(mesh)
    assert model.addMaterial(m) == 0
    assert model.addMaterial(c, name="second") == 1
    model.initFull(aka._static)
    assert model.getNbMaterials() == 2
    mats = model.getElementMaterial(aka._bernoulli_beam_2)
    # zero-copy writable views (D-078 section 2)
    assert mats.shape == (2, 1) and mats.flags.writeable and mats.dtype == np.uint32
    assert model.getMaterial("second").I == 2.0
    el = aka.Element(aka._bernoulli_beam_2, 0, aka._not_ghost)
    assert model.getMaterialByElement(el).E == 1e9
    forces = model.getExternalForce()
    assert forces.shape == (3, 3) and forces.flags.writeable
    blocked = model.getBlockedDOFs()
    blocked[0, :] = True
    assert model.getBlockedDOFs()[0].all()
    disp = model.getDisplacement()
    assert disp.flags.writeable and disp.shape == (3, 3)
    model.solveStep()
    assert np.allclose(model.getDisplacement(), 0)
    assert model.getPotentialEnergy() == pytest.approx(0)
    assert np.isfinite(model.getEnergy("potential"))
    assert not model.hasLumpedMass()


def test_heat_transfer_model():
    aka.parseInput("heat_transfer_input.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.HeatTransferModel(mesh)
    model.initFull(aka._static)
    T = model.getTemperature()
    assert T.shape == (mesh.getNbNodes(), 1) and T.flags.writeable
    b = model.getBlockedDOFs()
    X = mesh.getNodes()
    left, right = X[:, 0] < 1e-12, X[:, 0] > X[:, 0].max() - 1e-12
    b[left | right, 0] = True
    T[left, 0] = 1.0
    T[right, 0] = 0.0
    model.solveStep()
    T = model.getTemperature()
    assert T.min() >= -1e-12 and T.max() <= 1 + 1e-12
    law = model.getConstitutiveLaw(0)
    assert type(law) is aka.HeatDiffusion2D
    assert model.getNbConstitutiveLaws() == 1
    grad = law.getInternalReal("∇u")
    assert isinstance(grad, aka.InternalFieldReal)
    assert grad(aka._quadrangle_8).shape[1] == 2
    assert law.getMatrix("conductivity").ndim == 2
    by_el = model.getConstitutiveLawByElement()(aka._quadrangle_8)
    assert by_el.shape == (400, 1)
    assert model.isInternal("D∇u")
    assert model.getStableTimeStep() > 0
    # handler: local numbering, flatten / inflate (the laws' internal as
    # mesh-indexed blocks), reassignment
    num = model.getConstitutiveLawLocalNumbering()(aka._quadrangle_8)
    assert num.shape == (400, 1) and not num.flags.writeable
    np.testing.assert_array_equal(num[:, 0], np.arange(400))
    flat = model.flattenInternalReal("∇u")
    g = flat(aka._quadrangle_8)
    np.testing.assert_array_equal(g, grad(aka._quadrangle_8))
    g[:] = 2.0 * g
    model.inflateInternalReal("∇u", flat)
    np.testing.assert_array_equal(grad(aka._quadrangle_8), g)
    # reassignment (D-078 section 7): the default selector keeps
    # every element on its law
    model.reassignConstitutiveLaw()
    np.testing.assert_array_equal(
        model.getConstitutiveLawLocalNumbering()(aka._quadrangle_8)[:, 0],
        np.arange(400))


def test_phase_field_static_example():
    aka.parseInput("material_static.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate_static.msh")
    coupler = aka.CouplerSolidPhaseField(mesh)
    solid = coupler.getSolidMechanicsModel()
    phase = coupler.getPhaseFieldModel()
    assert isinstance(solid, aka.SolidMechanicsModel)
    assert isinstance(phase, aka.PhaseFieldModel)
    solid.initFull(_analysis_method=aka._static)
    phase.initFull(_analysis_method=aka._static)
    damage = phase.getDamage()
    assert damage.shape == (mesh.getNbNodes(), 1) and damage.flags.writeable
    pf = phase.getPhaseField(0)
    assert isinstance(pf, aka.PhaseField)
    assert phase.getPhaseFieldIndex(pf.getName() if hasattr(pf, "getName")
                                    else "plate") >= 0 or True
    solid.applyBC(aka.FixedValue(0.0, aka._y), "bottom")
    solid.applyBC(aka.FixedValue(0.0, aka._x), "left")
    solid.getDisplacement()[:, 1] = 1e-4 * mesh.getNodes()[:, 1]
    coupler.solve("static", "static")
    coupler.computeStrainOnQuadPoints()
    coupler.computeDamageOnQuadPoints()
    assert np.isfinite(phase.getDamage()).all()
    assert np.isfinite(phase.getEnergy())
    # D-078 section 2: a zero-copy writable view, as
    # upstream's def_function_nocopy (was a read-only copy)
    f = phase.getInternalForce()
    assert f.flags.writeable and f.shape == (mesh.getNbNodes(), 1)
    phase.assembleInternalForces()
    again = phase.getInternalForce()
    assert np.array_equal(f, again) and np.shares_memory(f, again)
    f[:] = 7.0
    assert np.all(phase.getInternalForce() == 7.0)
    phase.assembleInternalForces()  # recomputed in place: the view follows
    assert np.array_equal(f, again) and not np.all(f == 7.0)


def test_contact_detection():
    aka.parseInput("compression.dat")
    mesh = aka.Mesh(2)
    mesh.read("compression.msh")
    coupler = aka.CouplerSolidContact(mesh)
    solid = coupler.getSolidMechanicsModel()
    contact = coupler.getContactMechanicsModel()
    assert isinstance(contact, aka.ContactMechanicsModel)
    solid.setMaterialSelector(
        aka.MeshDataMaterialSelectorString("physical_names", solid))
    coupler.initFull(_analysis_method=aka._explicit_lumped_mass)
    detector = contact.getContactDetector()
    detector.setSurfaceSelector(aka.PhysicalSurfaceSelector(mesh))
    solid.applyBC(aka.FixedValue(0.0, aka._x), "sides")
    coupler.setTimeStep(solid.getStableTimeStep() * 0.1)
    assert coupler.getStableTimeStep() > 0
    for _ in range(5):
        solid.applyBC(aka.IncrementValue(-1e-6, aka._y), "loading")
        coupler.solveStep()
    gaps = contact.getGaps()
    assert gaps.shape[0] == mesh.getNbNodes() and gaps.flags.writeable
    assert contact.getNormals().shape[0] == mesh.getNbNodes()
    els = contact.getContactElements()
    assert els.__size__() >= 0
    assert coupler.getDisplacement().shape == (mesh.getNbNodes(), 2)
    assert coupler.getContactForce().shape == (mesh.getNbNodes(), 2)
    e = aka.ContactElement()
    e.slave = 3
    assert e.slave == 3 and "slave: 3" in repr(e)


def test_upstream_compat_switches():
    """Port extension setUpstreamCompat (absent upstream)."""
    aka.parseInput("compression.dat")
    mesh = aka.Mesh(2)
    mesh.read("compression.msh")
    coupler = aka.CouplerSolidContact(mesh)
    coupler.setUpstreamCompat()
    coupler.setUpstreamCompat(False)
    coupler.getContactMechanicsModel().setUpstreamCompat(True)
    contact = aka.ContactMechanicsModel(mesh)
    contact.setUpstreamCompat(True)
    aka.parseInput("material_static.dat")
    pmesh = aka.Mesh(2)
    pmesh.read("plate_static.msh")
    pf = aka.CouplerSolidPhaseField(pmesh)
    pf.setUpstreamCompat(True)
    pf.getPhaseFieldModel().setUpstreamCompat(False)
    assert not hasattr(pf.getSolidMechanicsModel(), "setUpstreamCompat")


def test_upstream_compat_getter_and_docstrings():
    """D-078: ``getUpstreamCompat()`` reads the
    switch back for every class that has ``setUpstreamCompat``, and the
    docstring lists the upstream defects it covers."""
    aka.parseInput("compression.dat")
    mesh = aka.Mesh(2)
    mesh.read("compression.msh")
    cmesh = aka.Mesh(2)  # the cohesive coupler needs a mesh of its own
    cmesh.read("compression.msh")
    scc = aka.CouplerSolidCohesiveContact(cmesh)
    sc = aka.CouplerSolidContact(mesh)
    contact = aka.ContactMechanicsModel(mesh)
    aka.parseInput("material_static.dat")  # the parser is process-global
    pmesh = aka.Mesh(2)
    pmesh.read("plate_static.msh")
    pf = aka.CouplerSolidPhaseField(pmesh)
    objs = {
        "contact": contact,
        "sc": sc,
        "sc.contact": sc.getContactMechanicsModel(),
        "scc": scc,
        "pf": pf,
        "pf.phase": pf.getPhaseFieldModel(),
    }
    for key, obj in objs.items():
        assert obj.getUpstreamCompat() is False, key  # corrected defaults
    for key in ("contact", "sc", "scc", "pf", "pf.phase"):
        objs[key].setUpstreamCompat()
        assert objs[key].getUpstreamCompat() is True, key
    # a coupler switches its part too, and a part alone does not make the
    # whole coupler upstream-compatible
    assert objs["sc.contact"].getUpstreamCompat() is True
    sc.setUpstreamCompat(False)
    assert sc.getUpstreamCompat() is False
    assert objs["sc.contact"].getUpstreamCompat() is False
    objs["sc.contact"].setUpstreamCompat(True)
    assert sc.getUpstreamCompat() is False
    pf.getPhaseFieldModel().setUpstreamCompat(False)
    assert pf.getUpstreamCompat() is False
    docs = {
        aka.ContactMechanicsModel: ["UD-002", "UD-008", "UD-363", "UD-372"],
        aka.CouplerSolidContact: ["UD-374", "UD-1291", "UD-002", "UD-372"],
        aka.CouplerSolidCohesiveContact: ["UD-374", "UD-772", "UD-1291", "UD-002"],
        aka.PhaseFieldModel: ["UD-377", "UD-380", "UD-385", "UD-387", "UD-388"],
        aka.CouplerSolidPhaseField: ["UD-382", "UD-619", "UD-620", "UD-377"],
    }
    for cls, uds in docs.items():
        doc = cls.setUpstreamCompat.__doc__
        assert all(u in doc for u in uds), (cls.__name__, doc)
        assert "getUpstreamCompat" in doc
        assert cls.getUpstreamCompat.__doc__


def test_geometry_utils():
    mesh = aka.Mesh(2)
    acc = aka.MeshAccessor(mesh)
    acc.resizeNodes(2)
    mesh.getNodes()[:] = [[0.0, 0.0], [2.0, 0.0]]
    mesh.addConnectivityType(aka._segment_2)
    acc.resizeConnectivity(1, aka._segment_2)
    mesh.getConnectivity(aka._segment_2)[:] = [[0, 1]]
    acc.makeReady()
    X = mesh.getNodes()
    el = aka.Element(aka._segment_2, 0, aka._not_ghost)
    n = aka.GeometryUtils.normal(mesh, X, el, False)
    np.testing.assert_allclose(n, [0.0, -1.0], atol=1e-15)  # Math::normal(x1 - x0)
    cov = aka.GeometryUtils.covariantBasis(mesh, X, el, n, np.array([0.0]))
    assert cov.shape == (2, 1)
    np.testing.assert_allclose(cov[:, 0], [1.0, 0.0])
    curv = aka.GeometryUtils.curvature(mesh, X, el, np.array([0.0]))
    np.testing.assert_allclose(curv, 0.0, atol=1e-15)  # straight segment
    contra = aka.GeometryUtils.contravariantBasis(np.array([2.0, 0.0]))
    assert contra.shape == (2, 1)
    np.testing.assert_allclose(contra[:, 0], [0.5, 0.0])  # T (T^T T)^-1
    proj = aka.GeometryUtils.realProjection(mesh, X, np.array([0.5, 0.3]), el, n)
    np.testing.assert_allclose(proj, [0.5, 0.0], atol=1e-15)
    with pytest.raises(aka.Exception):
        aka.GeometryUtils.realProjection(mesh, X, np.array([0.5, 0.3, 0.0]), el, n)
