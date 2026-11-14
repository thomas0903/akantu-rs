# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (groups 1 and 2, D-078 sections 2 and 4):
# python/py_structural_mechanics_model.cc (getExternalForce,
# getInternalForce, getElementMaterial by reference),
# python/py_contact_mechanics_model.cc (the contact fields by reference),
# python/py_model.cc (Model.setBaseName / addDumpField* / dump of the
# contact model and the couplers).
"""Zero-copy views of the structural and contact arrays; model dumpers."""

import os

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES, REPO

EX = "examples/python"


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "compression.msh": MESHES / EX
            / "contact_mechanics_model/compression/compression.msh",
            "compression.dat": PARSER_FIXTURES / EX
            / "contact_mechanics_model/compression/compression.dat",
            "detection-explicit.msh": REPO
            / "fixtures/mesh_io/examples/python/contact_mechanics_model/compression"
            / "detection-explicit/inputs/detection-explicit.msh",
            "detection-explicit.dat": PARSER_FIXTURES / EX
            / "contact_mechanics_model/compression/detection-explicit.dat",
            "plate_static.msh": MESHES / EX / "phase_field_model/plate_static.msh",
            "cohesive-contact.msh": REPO / "fixtures/meshes/gmsh-4.13.1/examples/c++"
            / "contact_mechanics_model/hertz/cohesive-contact.msh",
            "material-cohesive.dat": REPO
            / "crates/akantu-couplers/tests/data/contact/material-cohesive.dat",
            "material_static.dat": PARSER_FIXTURES / EX
            / "phase_field_model/material_static.dat",
        }
    )


def _beam(nb_elem=2):
    mesh = aka.Mesh(2)
    mesh.addConnectivityType(aka._bernoulli_beam_2)
    acc = aka.MeshAccessor(mesh)
    acc.resizeConnectivity(nb_elem, aka._bernoulli_beam_2)
    acc.resizeNodes(nb_elem + 1)
    mesh.getNodes()[:, :] = 0.0
    mesh.getNodes()[:, 0] = np.arange(nb_elem + 1, dtype=float)
    conn = mesh.getConnectivity(aka._bernoulli_beam_2)
    for e in range(nb_elem):
        conn[e, :] = [e, e + 1]
    acc.makeReady()
    model = aka.StructuralMechanicsModel(mesh)
    for e_modulus in (1e9, 2e9):
        mat = aka.StructuralMaterial()
        mat.E, mat.A, mat.I, mat.GJ, mat.rho = e_modulus, 1.0, 1.0, 1.0, 1.0
        model.addMaterial(mat)
    model.initFull(aka._static)
    blocked = model.getBlockedDOFs()
    blocked[0, :] = True
    return mesh, model


def test_structural_external_force_write_is_seen_by_the_solve():
    _, model = _beam()
    forces = model.getExternalForce()
    assert forces.flags.writeable and forces.base is not None
    forces[2, 0] = 100.0  # an axial point load, written into the view
    assert model.getExternalForce()[2, 0] == 100.0
    model.solveStep()
    u = model.getDisplacement()
    # two bars E A / L = 1e9 in series: u(1) = 1e-7, u(2) = 2e-7
    assert u[1, 0] == pytest.approx(1e-7, rel=1e-12)
    assert u[2, 0] == pytest.approx(2e-7, rel=1e-12)


def test_structural_internal_force_view_follows_the_model():
    _, model = _beam()
    fint = model.getInternalForce()  # taken once, before the solve
    model.getExternalForce()[2, 0] = 100.0
    model.solveStep()
    # the same view shows the internal forces of the converged step
    assert fint[2, 0] == pytest.approx(-100.0, rel=1e-12)
    assert np.array_equal(fint, model.getInternalForce())


def test_structural_element_material_write_changes_the_stiffness():
    _, model = _beam()
    materials = model.getElementMaterial(aka._bernoulli_beam_2)
    assert materials.dtype == np.uint32 and materials.flags.writeable
    model.getExternalForce()[2, 0] = 100.0
    model.solveStep()
    u_soft = model.getDisplacement()[2, 0]
    materials[:, :] = 1  # E = 2e9 on both elements, seen by the assembly
    assert (model.getElementMaterial(aka._bernoulli_beam_2) == 1).all()
    model.solveStep()
    assert model.getDisplacement()[2, 0] == pytest.approx(u_soft / 2, rel=1e-12)


def _compression():
    aka.parseInput("compression.dat")
    mesh = aka.Mesh(2)
    mesh.read("compression.msh")
    coupler = aka.CouplerSolidContact(mesh)
    solid = coupler.getSolidMechanicsModel()
    contact = coupler.getContactMechanicsModel()
    solid.setMaterialSelector(
        aka.MeshDataMaterialSelectorString("physical_names", solid))
    coupler.initFull(_analysis_method=aka._explicit_lumped_mass)
    contact.getContactDetector().setSurfaceSelector(aka.PhysicalSurfaceSelector(mesh))
    solid.applyBC(aka.FixedValue(0.0, aka._x), "sides")
    coupler.setTimeStep(solid.getStableTimeStep() * 0.1)
    return mesh, coupler, solid, contact


def test_contact_views_follow_every_search():
    mesh, coupler, solid, contact = _compression()
    gaps = contact.getGaps()  # taken once, as bloc_friction.py does
    normals = contact.getNormals()
    assert gaps.flags.writeable and normals.flags.writeable
    for _ in range(200):
        solid.applyBC(aka.IncrementValue(-1e-5, aka._y), "loading")
        coupler.solveStep()
        assert np.array_equal(gaps, contact.getGaps())
        assert np.array_equal(normals, contact.getNormals())
    assert gaps.shape == (mesh.getNbNodes(), 1)


def test_contact_gaps_and_areas_writes_are_seen_by_the_force_assembly():
    # upstream EX-044's set-up: one search on a committed mesh
    aka.parseInput("detection-explicit.dat")
    mesh = aka.Mesh(2)
    mesh.read("detection-explicit.msh")
    model = aka.ContactMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    model.getContactDetector().setSurfaceSelector(aka.PhysicalSurfaceSelector(mesh))
    gaps = model.getGaps()
    model.search()
    slaves = [e.slave for e in model.getContactElements()]
    assert slaves, "the search finds contact elements"
    gaps[slaves, 0] = 1e-3  # a penetration written into the view
    model.assembleInternalForces()
    force = model.getInternalForce().copy()
    assert np.abs(force).max() > 0
    areas = model.getNodalArea()
    areas *= 2.0  # the linear penalty force is linear in the nodal area
    model.assembleInternalForces()
    assert np.array_equal(model.getInternalForce(), 2.0 * force)


def _group_nodes(mesh, name):
    return sorted(int(n) for n in np.asarray(
        mesh.getElementGroup(name).getNodeGroup().getNodes()).ravel())


def test_surface_selectors_create_the_mesh_group_contact_surface():
    # EX-044 (line 37, "by default the contact model
    # creates a group named contact_surface"): upstream's
    # PhysicalSurfaceSelector creates the mesh group master + slave
    # (surface_selector.cc:37-52); the port mirrors the selector's surface
    # there when the selector is set, on a contact model alone and through
    # a coupler
    aka.parseInput("detection-explicit.dat")
    mesh = aka.Mesh(2)
    mesh.read("detection-explicit.msh")
    model = aka.ContactMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    with pytest.raises(Exception):
        mesh.getElementGroup("contact_surface")
    model.getContactDetector().setSurfaceSelector(aka.PhysicalSurfaceSelector(mesh))
    group = mesh.getElementGroup("contact_surface")
    expected = sorted(set(_group_nodes(mesh, "contact_top"))
                      | set(_group_nodes(mesh, "contact_bottom")))
    assert _group_nodes(mesh, "contact_surface") == expected
    assert len(group) == len(mesh.getElementGroup("contact_top")) + len(
        mesh.getElementGroup("contact_bottom"))
    model.search()
    assert list(model.getContactElements())
    # through a coupler (the solid model is a consumer of the mesh)
    mesh2, coupler, solid, contact = _compression()
    assert len(mesh2.getElementGroup("contact_surface")) > 0
    coupler.solveStep()


def _vtu_names(directory):
    return sorted(
        os.path.join(r, f)[len(directory) + 1:]
        for r, _, fs in os.walk(directory) for f in fs)


def test_coupler_and_contact_dumpers_are_not_refused(tmp_path):
    _, coupler, solid, contact = _compression()
    coupler.setDirectory(str(tmp_path / "c"))
    coupler.setBaseName("compression")
    for f in ("displacement", "contact_force", "external_force", "internal_force"):
        coupler.addDumpFieldVector(f)
    for f in ("gaps", "areas", "blocked_dofs", "grad_u", "stress"):
        coupler.addDumpField(f)
    coupler.dump()
    coupler.dump()
    names = _vtu_names(str(tmp_path / "c"))
    assert "compression_0001.pvtu" in names, names
    assert not any(n.endswith(".pvd") for n in names)  # coupler.setTimeStep
    # the contact model's own calls act on the same (default) series
    contact.addDumpFieldVector("normals")
    coupler.dump()
    with pytest.raises(aka.Exception):
        coupler.addDumpField("no_such_field")
    # upstream's contact.dump() writes the mesh's default dumper the coupler
    # filled, the solid's fields included (TEST-215's flow, UD-982)
    contact.dump()
    names = _vtu_names(str(tmp_path / "c"))
    assert "compression_0003.pvtu" in names, names


def test_coupler_part_dump_calls_reach_the_series_upstream_writes(tmp_path):
    # upstream's part calls act on the mesh's default dumper
    # E3 / E4: the solid part of a CouplerSolidContact: its setTimeStep
    # gives the coupler's series its time step (a .pvd), its
    # addDumpFieldVector puts the solid's field in that series
    _, coupler, solid, contact = _compression()
    coupler.setDirectory(str(tmp_path / "e3"))
    coupler.setBaseName("e3")
    coupler.addDumpField("gaps")
    solid.addDumpFieldVector("displacement")
    solid.setTimeStep(solid.getStableTimeStep() * 0.1)
    coupler.dump()
    solid.dump()  # the solid part's dump continues the same series
    names = _vtu_names(str(tmp_path / "e3"))
    assert "e3.pvd" in names and "e3_0001.pvtu" in names, names
    vtu = sorted(n for n in names if n.endswith(".vtu"))[-1]
    with open(os.path.join(str(tmp_path / "e3"), vtu), "rb") as fh:
        assert b'Name="displacement"' in fh.read()
    # E2: the phase-field part's own series never overwrites the coupler's
    aka.parseInput("material_static.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate_static.msh")
    pf = aka.CouplerSolidPhaseField(mesh)
    psolid = pf.getSolidMechanicsModel()
    phase = pf.getPhaseFieldModel()
    psolid.initFull(_analysis_method=aka._static)
    phase.initFull(_analysis_method=aka._static)
    pf.setDirectory(str(tmp_path / "pf"))
    phase.setDirectory(str(tmp_path / "pf"))
    pf.addDumpFieldVector("displacement")
    phase.addDumpField("damage")
    pf.dump()
    phase.dump()
    names = _vtu_names(str(tmp_path / "pf"))
    assert "phase_field_model_0000.pvtu" in names, names
    assert "phase_field_model_part_0000.pvtu" in names, names
    # E1: the cohesive solid of a CouplerSolidCohesiveContact dumps the
    # mesh's default dumper the coupler filled, contact fields included
    aka.parseInput("material-cohesive.dat")
    mesh4 = aka.Mesh(2)
    mesh4.read("cohesive-contact.msh")
    cc = aka.CouplerSolidCohesiveContact(mesh4)
    csolid = cc.getSolidMechanicsModelCohesive()
    cc.getContactMechanicsModel().getContactDetector().setSurfaceSelector(
        aka.CohesiveSurfaceSelector(mesh4))
    cc.initFull(_analysis_method=aka._explicit_lumped_mass, _is_extrinsic=True)
    cc.setDirectory(str(tmp_path / "cc"))
    cc.addDumpField("gaps")
    cc.dump()
    csolid.dump()
    names = _vtu_names(str(tmp_path / "cc"))
    assert sum(n.endswith(".pvtu") for n in names) == 2, names


def test_phase_field_coupler_dump_continues_the_solid_series(tmp_path):
    aka.parseInput("material_static.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate_static.msh")
    model = aka.CouplerSolidPhaseField(mesh)
    solid = model.getSolidMechanicsModel()
    phase = model.getPhaseFieldModel()
    solid.initFull(_analysis_method=aka._static)
    phase.initFull(_analysis_method=aka._static)
    solid.setDirectory(str(tmp_path))
    solid.setBaseName("plate")
    solid.addDumpFieldVector("displacement")
    solid.addDumpField("damage")
    solid.dump()
    model.dump()
    names = _vtu_names(str(tmp_path))
    assert "plate_0000.pvtu" in names and "plate_0001.pvtu" in names, names


def test_model_objects_are_parsable():
    # Upstream's ModelSolver derives from Parsable
    # (python/py_model.cc:134): the 14 Parsable methods on every model
    _, model = _beam()
    for name in ("get", "set", "getReal", "setReal", "getBool", "setBool",
                 "getInt", "setInt", "getString", "setString", "getMatrix",
                 "setRandom", "parseSection", "registerParamReal"):
        assert callable(getattr(model, name)), name
    with pytest.raises(aka.Exception):
        model.get("no_such_parameter")
    model.registerParamReal("alpha", aka._pat_modifiable, "a model parameter")
    model.setReal("alpha", 2.5)
    assert model.getReal("alpha") == 2.5
    assert model.get("alpha") == 2.5
    # the same registry on every call (one per model object)
    model.set("alpha", 3.0)
    assert model.getReal("alpha") == 3.0
