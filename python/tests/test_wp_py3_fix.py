# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests: upstream behaviour asserted from
# python/py_material.cc (PyMaterial trampoline: PYBIND11_OVERRIDE, i.e.
# pybind11's get_override frame rule), src/model/solid_mechanics/material.hh
# (hasMatrixChanged, lines 479-490), src/io/parser/parameter_registry_tmpl.hh
# (registerParam refuses a second registration, lines 374-386; isOfType,
# lines 98-108), python/py_parser.cc (ParameterRegistry.get, lines 88-110),
# python/py_solver.cc (SolverVector.getValues, a view of the residual).
# The same cases run on both modules in the oracle snippets
# python/oracle/snippets/{py3fix_items,v144_overrides,v144_internals}.py.
"""Fixes: trampoline rule inside callbacks, second
registrations, the mutant-killing cases, generic get of
non-Real parameters (UD-1174), hasMatrixChanged."""

import itertools

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES

CM = "examples/python/solid_mechanics_model/custom-material"
EX = "examples/python/solid_mechanics_cohesive_model"
_names = itertools.count()


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(**{
        "bar.msh": MESHES / CM / "bar.msh",
        "plate.msh": MESHES / EX / "cohesive/plate.msh",
        "elastic.dat": PARSER_FIXTURES / "test/test_python_interface/elastic.dat",
        "mesh_dcb_2d.msh": MESHES / "test/test_python_interface/mesh_dcb_2d.msh",
    })


INPUT = """
model solid_mechanics_model [
  sparse_solver_type = eigen
  material {name} [
    name = bar
    rho = 1.
    E = 1.
    nu = .3
  ]
]
"""


def register(cls, prefix="py3fix"):
    name = f"{prefix}_{next(_names)}"
    aka.MaterialFactory.getInstance().registerAllocator(
        name, lambda dim, opt, model, _id: cls(model, _id))
    return name


def bar(law, method):
    with open(f"{law}.dat", "w") as f:
        f.write(INPUT.format(name=law))
    aka.parseInput(f"{law}.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=method)
    model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    return mesh, model


def pull(mesh, model):
    x = mesh.getNodes()[:, 0]
    model.getExternalForce()[x <= x.min() + 1e-9, 0] = -1e-3


def linear_stress(mat, el_type, ghost_type):
    lam, mu = mat.getReal("lambda"), mat.getReal("mu")
    grad_u = mat.getGradU(el_type, ghost_type)
    sigma = mat.getStress(el_type, ghost_type)
    n = grad_u.shape[0]
    g = grad_u.reshape((n, 2, 2))
    eps = 0.5 * (g + np.einsum("aij->aji", g))
    trace = np.einsum("aii->a", g)
    sigma.reshape((n, 2, 2))[:, :, :] = (
        np.einsum("a,ij->aij", trace, lam * np.eye(2)) + 2.0 * mu * eps)


def first_element(mat):
    t = mat.getElementFilter().elementTypes()[0]
    return aka.Element(t, 0, aka._not_ghost)


# ---: pybind11's get_override inside a running callback -----------------


def test_p301_wave_speeds_inside_a_callback_run_the_override():
    """Inside computeStress (called by the model), ``super().getXWaveSpeed``
    and ``Material.getXWaveSpeed(self, ...)`` reach the trampoline, whose
    frame test fails (the frame is ``computeStress``), so the Python
    override answers, as ``self.getXWaveSpeed`` does."""
    seen = {}

    class Waves(aka.MaterialLinearElastic2D):
        def getPushWaveSpeed(self, element):
            return 0.25 * super().getPushWaveSpeed(element)

        def getShearWaveSpeed(self, element):
            return 3.0 * super().getShearWaveSpeed(element)

        def computeStress(self, el_type, ghost_type):
            linear_stress(self, el_type, ghost_type)
            if "p_super" not in seen and ghost_type == aka._not_ghost:
                el = aka.Element(el_type, 0, aka._not_ghost)
                seen["p_super"] = super().getPushWaveSpeed(el)
                seen["p_class"] = aka.Material.getPushWaveSpeed(self, el)
                seen["p_elastic"] = aka.MaterialLinearElastic2D.getPushWaveSpeed(self, el)
                seen["p_self"] = self.getPushWaveSpeed(el)
                seen["s_super"] = super().getShearWaveSpeed(el)
                seen["s_class"] = aka.Material.getShearWaveSpeed(self, el)
                seen["s_self"] = self.getShearWaveSpeed(el)

    name = register(Waves)
    mesh, model = bar(name, aka._explicit_lumped_mass)
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    model.solveStep()
    _, ref = bar("elastic", aka._explicit_lumped_mass)
    el = first_element(ref.getMaterial(0))
    base_p = ref.getMaterial(0).getPushWaveSpeed(el)
    base_s = ref.getMaterial(0).getShearWaveSpeed(el)
    for k in ("p_super", "p_class", "p_elastic", "p_self"):
        assert seen[k] == 0.25 * base_p, k
    for k in ("s_super", "s_class", "s_self"):
        assert seen[k] == 3.0 * base_s, k
    # outside the callbacks: the same answers (unchanged behaviour)
    mat = model.getMaterial(0)
    assert mat.getPushWaveSpeed(el) == 0.25 * base_p
    assert aka.Material.getShearWaveSpeed(mat, el) == 3.0 * base_s


def test_p301_stiffness_changed_inside_a_callback_runs_the_override():
    seen = {}

    class Frozen(aka.MaterialLinearElastic2D):
        def hasStiffnessMatrixChanged(self):
            seen["override_calls"] = seen.get("override_calls", 0) + 1
            return False

        def computeStress(self, el_type, ghost_type):
            linear_stress(self, el_type, ghost_type)
            if "super" not in seen:
                seen["super"] = super().hasStiffnessMatrixChanged()
                seen["class"] = aka.Material.hasStiffnessMatrixChanged(self)
                seen["K"] = super().hasMatrixChanged("K")

    name = register(Frozen)
    mesh, model = bar(name, aka._explicit_lumped_mass)
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    model.solveStep()
    assert seen["super"] is False and seen["class"] is False
    assert seen["K"] is False  # override answer, finite_deformation false
    assert seen["override_calls"] >= 3
    # outside the callbacks too (a top-level call reaches the override)
    before = seen["override_calls"]
    mat = model.getMaterial(0)
    assert mat.hasStiffnessMatrixChanged() is False
    assert aka.Material.hasStiffnessMatrixChanged(mat) is False
    assert mat.hasMatrixChanged("K") is False
    assert seen["override_calls"] == before + 3


def test_p301_init_material_dispatch():
    """``super().initMaterial()`` from another method of the class runs the
    override (whose own ``super().initMaterial()`` is the base's); a
    top-level ``initMaterial()`` runs the override once (its super-call is
    the base's, not the override again)."""
    calls = {"init": 0, "in_stress": 0}

    class Init(aka.MaterialLinearElastic2D):
        def initMaterial(self):
            calls["init"] += 1
            super().initMaterial()

        def computeStress(self, el_type, ghost_type):
            if calls["in_stress"] == 0:
                calls["in_stress"] = 1
                before = calls["init"]
                super().initMaterial()
                calls["delta"] = calls["init"] - before
            linear_stress(self, el_type, ghost_type)

    name = register(Init)
    mesh, model = bar(name, aka._static)
    assert calls["init"] == 1  # initFull
    pull(mesh, model)
    model.solveStep()
    assert calls["delta"] == 1
    mat = model.getMaterial(0)
    before = calls["init"]
    mat.initMaterial()
    assert calls["init"] == before + 1
    _, ref = bar("elastic", aka._static)
    pull(mesh, ref)
    ref.solveStep()
    assert np.array_equal(model.getDisplacement(), ref.getDisplacement())


def test_p301_no_override_answers_the_base_inside_a_callback():
    seen = {}

    class Plain(aka.MaterialLinearElastic2D):
        def computeStress(self, el_type, ghost_type):
            linear_stress(self, el_type, ghost_type)
            if "p" not in seen:
                el = aka.Element(el_type, 0, aka._not_ghost)
                seen["p"] = super().getPushWaveSpeed(el)
                seen["K"] = super().hasStiffnessMatrixChanged()

    name = register(Plain)
    _, model = bar(name, aka._explicit_lumped_mass)
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    model.solveStep()
    _, ref = bar("elastic", aka._explicit_lumped_mass)
    el = first_element(ref.getMaterial(0))
    assert seen["p"] == ref.getMaterial(0).getPushWaveSpeed(el)


# ---: a second registration of a base parameter ------------------------


@pytest.mark.parametrize("pname", ["E", "nu", "lambda"])
def test_p302_elastic_second_registration_raises(pname):
    class Dup(aka.MaterialLinearElastic2D):
        def __init__(self, model, _id):
            super().__init__(model, _id)
            self.registerParamReal(pname, aka._pat_parsmod, "dup")

    name = register(Dup)
    with pytest.raises(aka.Exception,
                       match=f"Parameter named {pname} already registered"):
        bar(name, aka._static)


def test_p302_a_new_parameter_is_still_accepted():
    class New(aka.MaterialLinearElastic2D):
        def __init__(self, model, _id):
            super().__init__(model, _id)
            self.registerParamReal(2.0, "my_factor", aka._pat_parsmod, "new")

    name = register(New)
    _, model = bar(name, aka._static)
    assert model.getMaterial(0).getReal("my_factor") == 2.0


COHESIVE_DAT = """model solid_mechanics_model_cohesive [
  cohesive_inserter [
    bounding_box = [[0,10],[-10, 10]]
  ]
  material elastic [
     name = virtual
     rho = 1
     E   = 1
     nu  = 0.3
  ]
  material {name} [
     name = spring
     sigma_c = 0.1
  ]
]
"""


def test_p302_cohesive_second_registration_raises():
    class DupSigma(aka.MaterialCohesive):
        def __init__(self, model, _id):
            super().__init__(model, _id)
            self.registerParamReal("sigma_c", aka._pat_parsmod, "dup")

        def computeTraction(self, el_type, ghost_type):
            pass

    name = register(DupSigma, "py3fix_coh")
    with open("coh.dat", "w") as f:
        f.write(COHESIVE_DAT.format(name=name))
    aka.parseInput("coh.dat")
    mesh = aka.Mesh(2)
    mesh.read("plate.msh")
    model = aka.SolidMechanicsModelCohesive(mesh)
    with pytest.raises(aka.Exception,
                       match="Parameter named sigma_c already registered"):
        model.initFull(_analysis_method=aka._explicit_lumped_mass,
                       _is_extrinsic=True)


# ---: the cases that catch the mutants M1, M3, M4 ----------------------


def test_p303_m1_retained_residual_view_through_implicit_dynamics():
    """The residual receives M·a (and C·v) through the matrix-product paths
    of the DOF manager: a view kept from before the solves equals a fresh
    one after every step (mutant M1 retires the exported block there)."""
    mesh, model = bar("elastic", aka._implicit_dynamic)
    pull(mesh, model)
    model.setTimeStep(0.05)
    res = model.getDOFManager().getResidual()
    v = res.getValues()
    for _ in range(3):
        model.solveStep()
        fresh = res.getValues()
        assert np.shares_memory(v, fresh)
        assert np.array_equal(v, fresh)
    assert np.abs(model.getDisplacement()).max() > 0.0
    assert np.abs(v).max() > 0.0


def test_p303_m3_tangent_override_is_called_and_exact():
    calls = {}

    class Tangent(aka.MaterialLinearElastic2D):
        def computeTangentModuli(self, el_type, tangent, ghost_type):
            calls["tangent"] = calls.get("tangent", 0) + 1
            calls["shape"] = tangent.shape
            lam, mu = self.getReal("lambda"), self.getReal("mu")
            d = np.array([[lam + 2 * mu, lam, 0.0], [lam, lam + 2 * mu, 0.0],
                          [0.0, 0.0, mu]])
            tangent.reshape((tangent.shape[0], 3, 3))[:] = d

    name = register(Tangent)
    mesh, model = bar(name, aka._static)
    pull(mesh, model)
    model.solveStep()
    assert calls["tangent"] >= 1 and calls["shape"][1] == 9
    _, ref = bar("elastic", aka._static)
    pull(mesh, ref)
    ref.solveStep()
    assert np.array_equal(model.getDisplacement(), ref.getDisplacement())


def test_p303_energy_and_shear_overrides():
    calls = {}

    def energy_class(value):
        class Energy(aka.MaterialLinearElastic2D):
            def computePotentialEnergy(self, el_type):
                calls["energy"] = calls.get("energy", 0) + 1
                self.getPotentialEnergy(el_type)[:] = value
        return Energy

    energies = []
    for value in (1.5, 3.0):
        name = register(energy_class(value))
        mesh, model = bar(name, aka._static)
        pull(mesh, model)
        model.solveStep()
        energies.append(model.getEnergy("potential"))
    assert calls["energy"] >= 2
    assert energies[0] > 0.0 and energies[1] == 2.0 * energies[0]

    class Shear(aka.MaterialLinearElastic2D):
        def getShearWaveSpeed(self, element):
            calls["shear"] = calls.get("shear", 0) + 1
            return 3.0 * super().getShearWaveSpeed(element)

    name = register(Shear)
    _, model = bar(name, aka._explicit_lumped_mass)
    _, ref = bar("elastic", aka._explicit_lumped_mass)
    mat = model.getMaterial(0)
    el = first_element(mat)
    base = ref.getMaterial(0).getShearWaveSpeed(el)
    assert mat.getShearWaveSpeed(el) == 3.0 * base
    assert aka.Material.getShearWaveSpeed(mat, el) == 3.0 * base
    assert calls["shear"] == 2


THERMAL = ("delta_T", "sigma_th", "epsilon_th")


def test_p303_m4_elastic_internals_inside_callbacks():
    """The elastic law's own internals (MaterialThermal's delta_T, sigma_th,
    epsilon_th) are reachable from the overrides of a subclass: in
    initMaterial and computeStress (state lent mutably: writable views of
    the law's storage) and in getPushWaveSpeed (state lent shared)."""
    seen = {}

    class Internals(aka.MaterialLinearElastic2D):
        def initMaterial(self):
            super().initMaterial()
            for name in THERMAL:
                for t in self.getElementFilter().elementTypes():
                    a = self.getInternalReal(name)(t)
                    seen[f"init.{name}"] = a.shape
                    seen[f"init.{name}.writeable"] = a.flags.writeable
                    if name == "delta_T":
                        a[:] = 0.0

        def computeStress(self, el_type, ghost_type):
            if "stress.delta_T" not in seen and ghost_type == aka._not_ghost:
                d = self.getInternalReal("delta_T")
                seen["stress.types"] = [int(t) for t in d.elementTypes()]
                seen["stress.delta_T"] = float(np.abs(d(el_type)).max())
            linear_stress(self, el_type, ghost_type)

        def getPushWaveSpeed(self, element):
            if "wave.delta_T" not in seen:
                seen["wave.delta_T"] = self.getInternalReal("delta_T")(
                    element.type).shape
            return super().getPushWaveSpeed(element)

    name = register(Internals)
    mesh, model = bar(name, aka._explicit_lumped_mass)
    model.setTimeStep(model.getStableTimeStep() * 0.5)
    model.solveStep()
    mat = model.getMaterial(0)
    t = mat.getElementFilter().elementTypes()[0]
    nq = mat.getStress(t).shape[0]
    assert seen["init.delta_T"] == (nq, 1)
    assert seen["init.sigma_th"] == (nq, 1)
    assert seen["init.epsilon_th"] == (nq, 1)
    assert all(seen[f"init.{n}.writeable"] for n in THERMAL)
    assert seen["stress.types"] == [int(t)]
    assert seen["stress.delta_T"] == 0.0
    assert seen["wave.delta_T"] == (nq, 1)


# --- / UD-1174: generic get of non-Real parameters ----------------------


def test_ud1174_generic_get_of_non_real_parameters():
    """Upstream's generic ``get`` raises for every non-Real parameter
    (UD-1174); the port returns the value with its type (category B)."""
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    mat = model.getMaterial(0)
    assert mat.get("Plane_Stress") is False
    assert mat.get("delta_T") == 0.0
    assert isinstance(mat.get("E"), float)
    for readable_only in ("finite_deformation", "lambda", "name"):
        with pytest.raises(aka.Exception, match="is not accessible"):
            mat.get(readable_only)


# ---: hasMatrixChanged --------------------------------------------------


def test_p306_has_matrix_changed_as_upstream():
    _, model = bar("elastic", aka._static)
    m = model.getMaterial(0)
    assert m.hasMatrixChanged("M") is False
    assert m.hasMatrixChanged("X") is True
    assert m.hasMatrixChanged("K") == m.hasStiffnessMatrixChanged()

    class Sub(aka.MaterialLinearElastic2D):
        def hasStiffnessMatrixChanged(self):
            return False

    name = register(Sub)
    _, model = bar(name, aka._static)
    s = model.getMaterial(0)
    assert s.hasMatrixChanged("M") is False
    assert s.hasMatrixChanged("K") is False  # override, no finite deformation
    assert s.hasMatrixChanged("X") is True
