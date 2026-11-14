# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (D-078): Python subclasses
# of the elastic laws (python/py_material.cc, `PyMaterial<MaterialElastic<
# dim>>`: the methods a subclass does not define are the elastic law's),
# of the diffusion laws (python/py_heat_transfer_model.cc) and of the phase
# fields (python/py_phasefield.cc). The comparison with the oracle module
# runs the snippets python/oracle/snippets/{elastic_subclass,
# law_subclasses}.py on both modules (docs/TESTING.md); these
# tests pin the port's behaviour with exact checks.
"""Python law subclasses: elastic base, diffusion, phase field."""

import itertools

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES, REPO

CM = "examples/python/solid_mechanics_model/custom-material"
_names = itertools.count()


def unique(prefix):
    """A fresh factory id (the factory is process-global, as upstream)."""
    return f"{prefix}_{next(_names)}"


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(**{
        "bar.msh": MESHES / CM / "bar.msh",
        "mesh_dcb_2d.msh": MESHES / "test/test_python_interface/mesh_dcb_2d.msh",
        "heat_transfer_input.dat": PARSER_FIXTURES
        / "test/test_model/patch_tests/data/heat_transfer_input.dat",
        "test_four_elements.msh": REPO
        / "crates/akantu-couplers/tests/data/test_phase_field_model/test_four_elements.msh",
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


def wave(mesh, model, steps=20):
    u = model.getDisplacement()
    x = mesh.getNodes()[:, 0] - 5.0
    k = 0.1 * 2 * np.pi * 3
    u[:, 0] = 0.01 * np.sin(k * x) * np.exp(-(k * x) * (k * x))
    dt = model.getStableTimeStep()
    model.setTimeStep(dt * 0.64)
    for _ in range(steps):
        model.solveStep()
    return dt, model.getDisplacement().copy(), model.getEnergy("potential")


def pull(mesh, model):
    """A traction on the free left end of the bar (XBlocked is the right
    end, YBlocked the top and bottom lines)."""
    x = mesh.getNodes()[:, 0]
    model.getExternalForce()[x <= x.min() + 1e-9, 0] = -1e-3


def register(cls):
    name = unique("py3_elastic")
    aka.MaterialFactory.getInstance().registerAllocator(
        name, lambda dim, opt, model, _id: cls(model, _id))
    return name


# --- PY-077: MaterialLinearElastic<d>D subclasses ------------------------------


def test_py077_a_subclass_without_overrides_is_the_elastic_law():
    class Plain(aka.MaterialLinearElastic2D):
        pass

    name = register(Plain)
    mesh, model = bar(name, aka._explicit_lumped_mass)
    mat = model.getMaterial(0)
    assert isinstance(mat, Plain) and isinstance(mat, aka.MaterialLinearElastic2D)
    assert mat.getReal("E") == 1.0 and mat.getReal("nu") == 0.3
    lam = mat.getReal("lambda")
    assert lam == 0.3 * 1.0 / ((1.0 + 0.3) * (1.0 - 2.0 * 0.3))
    got = wave(mesh, model)
    mesh_e, ref = bar("elastic", aka._explicit_lumped_mass)
    want = wave(mesh_e, ref)
    assert got[0] == want[0]
    assert np.array_equal(got[1], want[1])
    assert got[2] == want[2]


def test_py077_super_calls_run_the_elastic_implementation():
    calls = {"init": 0, "push": 0, "changed": 0}

    class InitAndWave(aka.MaterialLinearElastic2D):
        def __init__(self, model, _id):
            super().__init__(model, _id)
            self.registerParamReal(0.5, "factor", aka._pat_parsmod, "factor")
            self.registerInternalReal("my_field", 2)

        def initMaterial(self):
            calls["init"] += 1
            super().initMaterial()
            assert self.getReal("lambda") > 0.0  # computed by the base
            for t in self.getElementFilter().elementTypes():
                self.getInternalReal("my_field")(t)[:] = 3.0

        def getPushWaveSpeed(self, element):
            calls["push"] += 1
            return super().getPushWaveSpeed(element) * self.getReal("factor")

        def hasStiffnessMatrixChanged(self):
            calls["changed"] += 1
            return super().hasStiffnessMatrixChanged()

    name = register(InitAndWave)
    mesh, model = bar(name, aka._explicit_lumped_mass)
    mat = model.getMaterial(0)
    assert calls["init"] == 1
    assert mat.getReal("factor") == 0.5
    t = mat.getInternalReal("my_field").elementTypes()[0]
    assert np.all(mat.getInternalReal("my_field")(t) == 3.0)
    dt = model.getStableTimeStep()
    assert calls["push"] > 0
    mesh_e, ref = bar("elastic", aka._explicit_lumped_mass)
    # the wave speed is halved: the stable time step doubles, exactly
    assert dt == 2.0 * ref.getStableTimeStep()
    # outside the callbacks the methods dispatch to the override
    el = aka.Element(t, 0, aka._not_ghost)
    assert mat.getPushWaveSpeed(el) == 0.5 * ref.getMaterial(0).getPushWaveSpeed(el)
    # static: the base answers "changed" once (the first assembly)
    mesh_s, stat = bar(name, aka._static)
    pull(mesh_s, stat)
    stat.solveStep()
    stat.solveStep()
    assert calls["changed"] >= 2
    mesh_r, sref = bar("elastic", aka._static)
    pull(mesh_r, sref)
    sref.solveStep()
    sref.solveStep()
    assert np.abs(sref.getDisplacement()).max() > 0.0
    assert np.array_equal(stat.getDisplacement(), sref.getDisplacement())


def test_py077_python_stress_with_the_base_tangent_and_energies():
    calls = {"stress": 0}

    class PythonStress(aka.MaterialLinearElastic2D):
        def initMaterial(self):
            super().initMaterial()
            self.lam = self.getReal("lambda")
            self.mu = self.getReal("mu")

        def computeStress(self, el_type, ghost_type):
            calls["stress"] += 1
            grad_u = self.getGradU(el_type, ghost_type)
            sigma = self.getStress(el_type, ghost_type)
            n = grad_u.shape[0]
            g = grad_u.reshape((n, 2, 2))
            eps = 0.5 * (g + np.einsum("aij->aji", g))
            trace = np.einsum("aii->a", g)
            sigma.reshape((n, 2, 2))[:, :, :] = (
                np.einsum("a,ij->aij", trace, self.lam * np.eye(2))
                + 2.0 * self.mu * eps)

    name = register(PythonStress)
    mesh, model = bar(name, aka._static)
    pull(mesh, model)
    model.solveStep()
    assert calls["stress"] > 0
    u = model.getDisplacement()
    assert np.isfinite(u).all() and np.abs(u).max() > 0.0
    # the tangent and the energies are the elastic law's (the energy from
    # the Python stress); the values are compared with the oracle module by
    # the snippet elastic_subclass.py (bitwise)
    assert model.getEnergy("potential") > 0.0
    mat = model.getMaterial(0)
    s = mat.getStress(mat.getInternalReal("stress").elementTypes()[0])
    assert np.isfinite(s).all() and np.abs(s).max() > 0.0


def test_py077_a_subclass_of_another_dimension_is_refused():
    class Wrong(aka.MaterialLinearElastic3D):
        pass

    name = register(Wrong)
    with pytest.raises(aka.Exception, match="MaterialLinearElastic3D"):
        bar(name, aka._static)


# --- PY-083: HeatDiffusion<d>D subclasses ------------------------------------


def heat_model():
    aka.parseInput("heat_transfer_input.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.HeatTransferModel(mesh)
    model.initFull(aka._static)
    return mesh, model


def test_py083_a_heat_diffusion_subclass_is_constructible_and_never_called():
    """Upstream: `HeatDiffusion<d>D(model, id)` builds a law with the
    HeatDiffusion parameters that nothing adds to the model (no allocator
    of the diffusion factory is bound): its overrides are never called."""
    calls = []

    class MyHeat(aka.HeatDiffusion2D):
        def __init__(self, model, _id):
            super().__init__(model, _id)
            self.registerParamReal(2.5, "extra", aka._pat_parsmod, "extra")
            self.field = self.registerInternalReal("my_field", 1)

        def computeDiffusivityOnQuadPoints(self, el_type, ghost_type):
            calls.append("diffusivity")

        def getStableTimeStep(self, element_size):
            calls.append("dt")
            return 1.0

    mesh, model = heat_model()
    n = model.getNbConstitutiveLaws()
    h = MyHeat(model, "mine")
    assert isinstance(h, aka.HeatDiffusion2D) and isinstance(h, aka.DiffusionLaw)
    assert h.getSpatialDimension() == 2
    assert h.getReal("density") == 0.0 and h.getReal("capacity") == 0.0
    # upstream crashes here (updateInternalParameters reads the empty
    # conductivity matrix: segfault in the oracle); the port refuses (UD-285)
    with pytest.raises(aka.Exception, match="UD-285"):
        h.setReal("density", 3.0)
    h.set("conductivity", np.eye(2))
    h.setReal("density", 3.0)
    assert h.getReal("density") == 3.0
    assert h.getReal("extra") == 2.5
    assert model.getNbConstitutiveLaws() == n
    model.assembleInternalHeatRate() if hasattr(model, "assembleInternalHeatRate") else None
    assert calls == []
    with pytest.raises(TypeError):
        MyHeat(None, "x")
    # DiffusionLaw has no constructor upstream
    class Bare(aka.DiffusionLaw):
        pass
    with pytest.raises(TypeError):
        Bare(model, "y")


# --- PY-092: PhaseField subclasses and PhaseFieldFactory allocators ------------


def snippet(name):
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        name, REPO / "python/oracle/snippets" / f"{name}.py")
    return spec


def test_py092_a_python_phase_field_drives_the_damage(tmp_path):
    """An AT2 law without energy split written in NumPy, registered with
    `PhaseFieldFactory.registerAllocator` and named in the input file: the
    model calls its `computeDrivingForce` (not-ghost elements) and
    `computeDissipatedEnergy`; the views it writes are the law's
    internals; `getPhaseField(0)` is the Python object. The oracle
    comparison is the snippet phasefield_subclass.py (bitwise)."""
    calls = {"driving": 0, "dissipated": 0}
    name = unique("py3_at2")

    class PyAT2(aka.PhaseField):
        def __init__(self, model, _id):
            super().__init__(model, _id)
            self.registerParamReal(1.0, "scale", aka._pat_parsmod, "scale")
            self.registerInternalReal("psi", 1)

        def computeDrivingForce(self, el_type, ghost_type):
            calls["driving"] += 1
            E, nu = self.getReal("E"), self.getReal("nu")
            l0, gc = self.getReal("l0"), self.getReal("gc")
            lam = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
            mu = E / (2.0 * (1.0 + nu))
            strain = self.getStrain(el_type, ghost_type)
            n = strain.shape[0]
            if n == 0:
                return
            eps = strain.reshape((n, 2, 2))
            tr = np.einsum("aii->a", eps)
            psi = 0.5 * lam * tr * tr + mu * np.einsum("aij,aij->a", eps, eps)
            self.getInternalReal("psi")(el_type, ghost_type)[:, 0] = psi
            phi = self.getInternalReal("phi")(el_type, ghost_type)[:, 0]
            phi[:] = np.maximum(phi, psi)
            d = self.getDamage(el_type, ghost_type)[:, 0]
            gradd = self.getInternalReal("grad_d")(el_type, ghost_type)
            self.getInternalReal("driving_force")(el_type, ghost_type)[:, 0] = (
                (2.0 * phi + gc / l0) * d - 2.0 * phi)
            self.getInternalReal("driving_energy")(el_type, ghost_type)[:, :] = (
                gc * l0 * gradd)
            de = self.getInternalReal("damage_energy")(el_type, ghost_type)
            de.reshape((n, 2, 2))[:, :, :] = gc * l0 * np.eye(2)
            self.getInternalReal("damage_energy_density")(el_type, ghost_type)[
                :, 0] = 2.0 * phi + gc / l0

        def computeDissipatedEnergy(self, el_type):
            calls["dissipated"] += 1
            self.getInternalReal("dissipated_energy")(el_type)[:, 0] = 1.0

    factory = aka.PhaseFieldFactory.getInstance()
    factory.registerAllocator(name, lambda dim, split, model, _id: PyAT2(model, _id))
    with pytest.raises(aka.Exception):
        factory.registerAllocator(name, lambda *a: None)
    with pytest.raises(aka.Exception):
        factory.registerAllocator("quadratic", lambda *a: None)
    (tmp_path / "pf.dat").write_text(f"""
model solid_mechanics_model [
   material phasefield no_split [
     name = plate
     rho = 1.
     E = 210.0
     nu = 0.3
   ]
]
model phase_field_model [
   phasefield {name} no_split [
     name = plate
     E = 210.0
     nu = 0.3
     gc = 5e-3
     l0 = 0.3
     scale = 2.0
   ]
]
""")
    aka.parseInput(str(tmp_path / "pf.dat"))
    mesh = aka.Mesh(2)
    mesh.read("test_four_elements.msh")
    coupler = aka.CouplerSolidPhaseField(mesh)
    solid = coupler.getSolidMechanicsModel()
    phase = coupler.getPhaseFieldModel()
    eigen = aka.ModelOptions(aka._static, aka.ModelSolverOptions(
        sparse_solver_type=aka.SparseSolverType.eigen))
    solid.initFull(eigen)
    phase.initFull(eigen)
    pf = phase.getPhaseField(0)
    assert isinstance(pf, PyAT2)
    assert pf.getReal("scale") == 2.0 and pf.getReal("l0") == 0.3
    X = mesh.getNodes()
    top = X[:, 1] > X[:, 1].max() - 1e-12
    bottom = X[:, 1] < X[:, 1].min() + 1e-12
    left = X[:, 0] < X[:, 0].min() + 1e-12
    blocked = solid.getBlockedDOFs()
    blocked[bottom, 1] = blocked[top, 1] = True
    blocked[left, 0] = True
    u = solid.getDisplacement()
    for _ in range(3):
        phase.savePreviousState()
        u[top, 1] += 2e-3
        coupler.solve("static", "static")
    assert calls["driving"] > 0
    d = phase.getDamage()
    assert np.isfinite(d).all() and d.max() > 0.0
    t = aka._quadrangle_4 if aka._quadrangle_4 in pf.getInternalReal(
        "phi").elementTypes() else pf.getInternalReal("phi").elementTypes()[0]
    phi = pf.getInternalReal("phi")(t)
    psi = pf.getInternalReal("psi")(t)
    assert np.all(phi >= psi) and phi.max() > 0.0
    assert phase.getEnergy("dissipated") > 0.0 and calls["dissipated"] > 0
    with pytest.raises(TypeError):
        PyAT2(None, "x")
