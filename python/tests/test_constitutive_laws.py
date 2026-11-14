# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (group 2): the constitutive-law layer of the Python
# interface — python/py_material.cc (Material, PyMaterial trampoline,
# MaterialLinearElastic*D, MaterialFactory), python/py_constitutive_law.cc
# (InternalFieldReal/Int, ConstitutiveLawInternalHandler),
# python/py_constitutive_law.hh (ConstitutiveLaw<…>.set*),
# python/py_constitutive_laws_handler.hh (ConstitutiveLawsHandler<Material,
# Model>), python/py_constitutive_law_selector.cc, registerCustomMaterials of
# python/akantu/__init__.py. The Python laws follow upstream
# examples/python/solid_mechanics_model/custom-material/{custom-material.py,
# bi-material.py}.
"""Constitutive laws: Python materials, internals, factory, selectors."""

import itertools

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES

CM = "examples/python/solid_mechanics_model/custom-material"
_names = itertools.count()


def unique(prefix):
    """A fresh factory id (the factory is process-global, as upstream)."""
    return f"{prefix}_{next(_names)}"


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "square.msh": MESHES / CM / "square.msh",
            "bar.msh": MESHES / CM / "bar.msh",
            "mesh_dcb_2d.msh": MESHES
            / "test/test_python_interface/mesh_dcb_2d.msh",
            "elastic.dat": PARSER_FIXTURES
            / "test/test_python_interface/elastic.dat",
        }
    )


class LocalElastic(aka.Material):
    """Upstream bi-material.py's law (factor 0.5 below y = 0.5)."""

    def __init__(self, model, _id):
        super().__init__(model, _id)
        super().registerParamReal(
            "E", aka._pat_readable | aka._pat_parsmod, "Youngs modulus")
        super().registerParamReal(
            "nu", aka._pat_readable | aka._pat_parsmod, "Poisson ratio")
        self.factor = super().registerInternalReal("factor", 1)
        self.quad_coords = super().registerInternalReal("quad_coordinates", 2)
        self.calls = {"stress": 0, "tangent": 0, "energy": 0}

    def initMaterial(self):
        nu = self.getReal("nu")
        E = self.getReal("E")
        self.mu = E / (2 * (1 + nu))
        self.lame_lambda = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
        self.lame_mu = E / (2.0 * (1.0 + nu))
        super().initMaterial()
        model = self.getModel()
        model.getFEEngine().computeIntegrationPointsCoordinates(
            self.quad_coords, self.getElementFilter())
        for elem_type in self.factor.elementTypes():
            factor = self.factor(elem_type)
            coords = self.quad_coords(elem_type)
            factor[:] = 1.0
            factor[coords[:, 1] < 0.5] = 0.5

    def getPushWaveSpeed(self, params):
        return np.sqrt((self.lame_lambda + 2 * self.lame_mu)
                       / self.getReal("rho"))

    @staticmethod
    def computeEpsilon(grad_u):
        return 0.5 * (grad_u + np.einsum("aij->aji", grad_u))

    def computeStress(self, el_type, ghost_type):
        self.calls["stress"] += 1
        grad_u = self.getGradU(el_type, ghost_type)
        sigma = self.getStress(el_type, ghost_type)
        n_quads = grad_u.shape[0]
        grad_u = grad_u.reshape((n_quads, 2, 2))
        factor = self.getInternalReal("factor")(el_type, ghost_type).reshape(
            n_quads)
        epsilon = self.computeEpsilon(grad_u)
        sigma = sigma.reshape((n_quads, 2, 2))
        trace = np.einsum("aii->a", grad_u)
        sigma[:, :, :] = (
            np.einsum("a,ij->aij", trace, self.lame_lambda * np.eye(2))
            + 2.0 * self.lame_mu * epsilon)
        sigma[:, :, :] = np.einsum("aij, a->aij", sigma, factor)

    def computeTangentModuli(self, el_type, tangent_matrix, ghost_type):
        self.calls["tangent"] += 1
        n_quads = tangent_matrix.shape[0]
        tangent = tangent_matrix.reshape(n_quads, 3, 3)
        factor = self.getInternalReal("factor")(el_type, ghost_type).reshape(
            n_quads)
        Miiii = self.lame_lambda + 2 * self.lame_mu
        Miijj = self.lame_lambda
        Mijij = self.lame_mu
        tangent[:, 0, 0] = Miiii
        tangent[:, 1, 1] = Miiii
        tangent[:, 0, 1] = Miijj
        tangent[:, 1, 0] = Miijj
        tangent[:, 2, 2] = Mijij
        tangent[:, :, :] = np.einsum("aij, a->aij", tangent, factor)

    def computePotentialEnergy(self, el_type):
        self.calls["energy"] += 1
        sigma = self.getStress(el_type)
        grad_u = self.getGradU(el_type)
        nquads = sigma.shape[0]
        stress = sigma.reshape(nquads, 2, 2)
        grad_u = grad_u.reshape((nquads, 2, 2))
        epsilon = self.computeEpsilon(grad_u)
        energy_density = self.getPotentialEnergy(el_type)
        energy_density[:, 0] = 0.5 * np.einsum("aij,aij->a", stress, epsilon)


def apply_bc(model):
    """Upstream bi-material.py's applyBC."""
    nb_nodes = model.getMesh().getNbNodes()
    position = model.getMesh().getNodes()
    displacement = model.getDisplacement()
    blocked_dofs = model.getBlockedDOFs()
    for node in range(0, nb_nodes):
        if (np.abs(position[node, 0]) < 1e-8) or (
                np.abs(position[node, 0] - 1.0) < 1e-8):
            blocked_dofs[node, 0] = True
            displacement[node, 0] = 0.0
        if np.abs(position[node, 1]) < 1e-8:
            blocked_dofs[node, 1] = True
            displacement[node, 1] = -1.0
        if np.abs(position[node, 1] - 1.0) < 1e-8:
            blocked_dofs[node, 1] = True
            displacement[node, 1] = 1.0


def material_file(ty, E=1.0):
    """Parse a material file declaring one law of type `ty` (the static
    parser is process-global: the dict form of `initFull`, which upstream's
    scripts use without parsing a file, is tested in a fresh interpreter by
    `test_bi_material_dict_form_in_a_fresh_interpreter`)."""
    with open("law.dat", "w") as fh:
        fh.write(f"material {ty} [\n name = fictive\n rho = 1.\n"
                 f" E = {E}\n nu = 0.3\n]\n")
    aka.parseInput("law.dat")


def law_model(ty, mesh_file="square.msh", method=aka._static):
    material_file(ty)
    mesh = aka.Mesh(2)
    mesh.read(mesh_file)
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=method)
    return mesh, model


def bi_material_model(name):
    aka.registerCustomMaterials({name: LocalElastic})
    return law_model(name)


# --- class hierarchy ---------------------------------------------------------


def test_hierarchy_and_upstream_class_names():
    cl = getattr(aka, "ConstitutiveLawakantu::SolidMechanicsModel")
    clh = getattr(aka, "ConstitutiveLawsHandlerakantu::Materialakantu::Model")
    assert cl.__name__ == "ConstitutiveLawakantu::SolidMechanicsModel"
    assert issubclass(aka.Material, cl)
    assert issubclass(cl, aka.ConstitutiveLawInternalHandler)
    assert issubclass(aka.ConstitutiveLawInternalHandler, aka.Parsable)
    assert issubclass(aka.MaterialLinearElastic2D, aka.Material)
    assert issubclass(aka.SolidMechanicsModel, clh)
    assert issubclass(clh, aka.Model)
    assert issubclass(aka.InternalFieldReal, aka.ElementTypeMapArrayReal)
    assert issubclass(aka.InternalFieldInt, aka.ElementTypeMapArrayInt)
    assert issubclass(aka.MeshDataMaterialSelectorString,
                      aka.ConstitutiveLawSelector)
    assert issubclass(aka.DefaultMaterialSelector, aka.ConstitutiveLawSelector)
    assert not hasattr(aka, "ConstitutiveLaw_SolidMechanicsModel")


def test_built_in_laws_and_handler_methods():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    mat = model.getMaterial(0)
    assert type(mat) is aka.MaterialLinearElastic2D
    assert type(model.getMaterial("bulk")) is aka.MaterialLinearElastic2D
    el = aka.Element(aka._quadrangle_8, 3, aka._not_ghost)
    assert model.getConstitutiveLaw(el).getReal("E") == 71e9
    assert model.getNbConstitutiveLaws() == 1
    assert model.getConstitutiveLawIndex("bulk") == 0
    laws = model.getConstitutiveLaws()
    assert len(laws) == 1 and laws[0].getReal("nu") == 0.22
    by_el = model.getConstitutiveLawByElement()(aka._quadrangle_8)
    assert by_el.shape == (400, 1) and not by_el.any()
    num = model.getConstitutiveLawLocalNumbering()(aka._quadrangle_8)
    assert np.array_equal(num[:, 0], np.arange(400))
    assert not num.flags.writeable
    assert model.isInternal("stress") and not model.isInternal("bogus")
    # flatten / inflate
    model.getDisplacement()[:, 0] = 1e-3 * mesh.getNodes()[:, 0]
    model.assembleInternalForces()
    flat = model.flattenInternalReal("stress")
    s = flat(aka._quadrangle_8)
    assert s.shape == mat.getStress(aka._quadrangle_8).shape == (400 * 4, 4)
    assert np.array_equal(s, mat.getStress(aka._quadrangle_8))
    s[:] = 2.0 * s
    model.inflateInternalReal("stress", flat)
    assert np.array_equal(mat.getStress(aka._quadrangle_8), s)
    # registerNewConstitutiveLaw returns the law; registerNewMaterial None
    law = model.registerNewConstitutiveLaw("other", "elastic")
    assert isinstance(law, aka.Material)
    assert model.registerNewMaterial("third", "elastic") is None
    assert model.getNbMaterials() == 3
    # ConstitutiveLaw<…>.set*
    law.setReal("E", 3.0)
    law.setReal(name="rho", value=7.0)
    assert law.getReal("E") == 3.0 and law.getReal("rho") == 7.0
    with pytest.raises(aka.Exception):
        law.setReal("bogus", 1.0)


# --- Python materials -------------------------------------------------------------


def test_bi_material_example():
    name = unique("local_elastic")
    mesh, model = bi_material_model(name)
    mat = model.getMaterial(0)
    assert isinstance(mat, LocalElastic)  # the Python object itself
    assert mat is model.getMaterial("fictive")
    assert mat.lame_mu == pytest.approx(1.0 / 2.6)
    factor = mat.factor(aka._triangle_3)
    coords = mat.quad_coords(aka._triangle_3)
    assert factor.shape[0] == coords.shape[0] > 0
    assert np.array_equal(factor[:, 0] == 0.5, coords[:, 1] < 0.5)
    assert set(np.unique(factor)) <= {0.5, 1.0}
    solver = model.getNonLinearSolver()
    solver.set("max_iterations", 2)
    solver.set("threshold", 1e-3)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)
    apply_bc(model)
    model.solveStep()
    assert mat.calls["tangent"] >= 1 and mat.calls["stress"] >= 1
    u = model.getDisplacement()
    assert np.isfinite(u).all()
    epot = model.getEnergy("potential")
    assert mat.calls["energy"] >= 1
    assert np.isfinite(epot) and epot > 0
    # internals outside callbacks: views of the law's storage
    st = mat.getStress(aka._triangle_3)
    assert st.flags.writeable and st.shape == (factor.shape[0], 4)
    assert mat.getInternalReal("factor")(aka._triangle_3)[0, 0] in (0.5, 1.0)
    assert mat.getReal("E") == 1.0
    assert mat.getPushWaveSpeed(aka.Element(aka._triangle_3, 0,
                                            aka._not_ghost)) > 0


class AnsweringElastic(LocalElastic):
    """LocalElastic with upstream's overridable ``hasStiffnessMatrixChanged``
    (``PYBIND11_OVERRIDE(bool, _Material, hasStiffnessMatrixChanged)``,
    python/py_material.cc:85-88)."""

    answer = None

    def hasStiffnessMatrixChanged(self):
        return True if AnsweringElastic.answer is None else AnsweringElastic.answer


@pytest.mark.parametrize("answer", [False, True])
def test_has_stiffness_matrix_changed_override_is_honoured(answer):
    """Program A4: after a first static solve with
    the base answer, the law answers ``answer`` for two more solves; ``K``
    is re-assembled exactly when the law says it changed (oracle: release
    changes ``[False, False]`` for "unchanged", ``[True, True]`` for
    "changed"; the port re-assembled always before the fix)."""
    name = unique("answering")
    aka.registerCustomMaterials({name: AnsweringElastic})
    AnsweringElastic.answer = None
    try:
        material_file(name)
        mesh = aka.Mesh(2)
        mesh.read("bar.msh")
        model = aka.SolidMechanicsModel(mesh)
        model.initFull(_analysis_method=aka._static)
        model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
        model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
        X = mesh.getNodes()
        model.getExternalForce()[X[:, 0] > X[:, 0].max() - 1e-9, 1] = 1e-3
        model.getNonLinearSolver().set("max_iterations", 20)
        releases = []
        for k in range(3):
            if k == 1:
                AnsweringElastic.answer = answer
            model.solveStep()
            releases.append(model.getDOFManager().getMatrix("K").getRelease())
    finally:
        AnsweringElastic.answer = None
    assert [b != a for a, b in zip(releases, releases[1:])] == [answer, answer]
    assert np.isfinite(model.getDisplacement()).all()


def test_has_stiffness_matrix_changed_exception_reaches_solve_step():
    """An exception of the override leaves ``solveStep`` with its
    type and message (upstream's ``error_already_set``)."""

    class Raising(LocalElastic):
        def hasStiffnessMatrixChanged(self):
            raise ArithmeticError("from hasStiffnessMatrixChanged")

    name = unique("raising")
    aka.registerCustomMaterials({name: Raising})
    mesh, model = law_model(name)
    apply_bc(model)
    with pytest.raises(ArithmeticError, match="from hasStiffnessMatrixChanged"):
        model.solveStep()


def test_get_internal_of_a_missing_name_raises_at_once():
    """``getInternalReal("bogus")`` raises
    ``akantu.Exception`` at once, as upstream's ``getSharedPtrInternal``
    ("does not contain an internal"), for a built-in and a Python law;
    existing names still give the internal field object."""
    name = unique("local_elastic")
    mesh, model = bi_material_model(name)
    mat = model.getMaterial(0)
    for m in (mat,):
        with pytest.raises(aka.Exception, match="does not contain an internal bogus"):
            m.getInternalReal("bogus")
        with pytest.raises(aka.Exception, match="does not contain an internal"):
            m.getInternalInt("factor")  # a real internal asked as int
        assert m.getInternalReal("factor")(aka._triangle_3).shape[1] == 1
    material_file("elastic")
    mesh2 = aka.Mesh(2)
    mesh2.read("square.msh")
    model2 = aka.SolidMechanicsModel(mesh2)
    model2.initFull(_analysis_method=aka._static)
    builtin = model2.getMaterial(0)
    with pytest.raises(aka.Exception, match="does not contain an internal bogus"):
        builtin.getInternalReal("bogus")
    assert builtin.getInternalReal("stress")(aka._triangle_3).shape[1] == 4


def test_get_material_keyword_forms():
    """Upstream's three ``getMaterial`` overloads take
    ``material_id``, ``material_name`` or ``element`` by keyword."""
    material_file("elastic")
    mesh = aka.Mesh(2)
    mesh.read("square.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    by_pos = model.getMaterial(0)
    assert type(model.getMaterial(material_id=0)).__name__ == "MaterialLinearElastic2D"
    assert model.getMaterial(material_name="fictive").getReal("E") == by_pos.getReal("E")
    el = aka.Element(aka._triangle_3, 0, aka._not_ghost)
    assert model.getMaterial(element=el).getReal("E") == by_pos.getReal("E")
    with pytest.raises(aka.Exception):
        model.getMaterial(material_name="bogus")
    with pytest.raises(TypeError):
        model.getMaterial(material_id="fictive")
    with pytest.raises(TypeError):
        model.getMaterial(0, material_id=0)
    with pytest.raises(TypeError):
        model.getMaterial()


BI_MATERIAL_SCRIPT = r"""
import sys
import numpy as np
import akantu as aka
sys.path.insert(0, {here!r})
from test_constitutive_laws import LocalElastic, apply_bc

aka.registerCustomMaterials({{"local_elastic": LocalElastic}})
mesh = aka.Mesh(2)
mesh.read("square.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(
    {{"fictive": {{"type": "local_elastic", "rho": 1.0, "E": 1.0, "nu": 0.3}}}},
    _analysis_method=aka._static,
)
solver = model.getNonLinearSolver()
solver.set("max_iterations", 2)
solver.set("threshold", 1e-3)
solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)
apply_bc(model)
model.solveStep()
mat = model.getMaterial(0)
assert isinstance(mat, LocalElastic)
print("epot", model.getEnergy("potential"))
"""


def test_bi_material_dict_form_in_a_fresh_interpreter(tmp_path):
    import os
    import subprocess
    import sys

    here = os.path.dirname(os.path.abspath(__file__))
    script = tmp_path / "bi.py"
    script.write_text(BI_MATERIAL_SCRIPT.format(here=here))
    # the akantu package this test imported (not another installation)
    env = dict(os.environ)
    env["PYTHONPATH"] = os.pathsep.join(
        [os.path.dirname(os.path.dirname(aka.__file__)),
         env.get("PYTHONPATH", "")])
    r = subprocess.run([sys.executable, str(script)], capture_output=True,
                       text=True, cwd=os.getcwd(), timeout=300, env=env)
    assert r.returncode == 0, r.stderr
    epot = float(r.stdout.split()[-1])
    assert np.isfinite(epot) and epot > 0


def test_python_material_matches_built_in_elastic_explicit():
    name = unique("local_elastic_explicit")

    class Plain(LocalElastic):
        def initMaterial(self):
            nu, E = self.getReal("nu"), self.getReal("E")
            self.lame_lambda = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
            self.lame_mu = E / (2.0 * (1.0 + nu))
            aka.Material.initMaterial(self)
            for t in self.factor.elementTypes():
                self.factor(t)[:] = 1.0

    aka.getStaticParser()  # the factory is global
    aka.MaterialFactory.getInstance().registerAllocator(
        name, lambda dim, option, model, id_: Plain(model, id_))
    assert name in aka.MaterialFactory.getInstance().getPossibleAllocators()
    results = []
    for ty in (name, "elastic"):
        mesh, model = law_model(ty, "bar.msh", aka._explicit_lumped_mass)
        model.applyBC(aka.FixedValue(0, aka._x), "XBlocked")
        model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
        x = mesh.getNodes()[:, 0] - 5.0
        model.getDisplacement()[:, 0] = 0.01 * np.sin(1.88 * x) * np.exp(
            -(1.88 * x) ** 2)
        model.setTimeStep(model.getStableTimeStep() * 0.64)
        for _ in range(20):
            model.solveStep()
        results.append((model.getDisplacement().copy(),
                        model.getEnergy("potential")))
    (u_py, e_py), (u_rs, e_rs) = results
    assert np.allclose(u_py, u_rs, rtol=1e-12, atol=1e-15)
    assert e_py == pytest.approx(e_rs, rel=1e-12)


def test_callback_exception_propagates_with_its_type():
    name = unique("broken")

    class Broken(LocalElastic):
        def computeStress(self, el_type, ghost_type):
            raise ArithmeticError("stress failed")

    aka.registerCustomMaterials({name: Broken})
    mesh, model = law_model(name)
    apply_bc(model)
    with pytest.raises(ArithmeticError, match="stress failed"):
        model.solveStep()


def test_handles_die_with_the_callback():
    name = unique("keeper")
    kept = {}

    class Keeper(LocalElastic):
        def initMaterial(self):
            super().initMaterial()
            kept["stress"] = self.getStress(aka._triangle_3)
            kept["model"] = self.getModel()

    aka.registerCustomMaterials({name: Keeper})
    mesh, model = law_model(name)
    # the view taken inside the callback is a view of the live law array
    mat = model.getMaterial(0)
    mat.getStress(aka._triangle_3)[0, 0] = 42.0
    assert kept["stress"][0, 0] == 42.0
    # the model object obtained inside the callback works afterwards
    assert kept["model"].getNbMaterials() == 1


def test_material_factory_errors_and_unattached_objects():
    f = aka.MaterialFactory.getInstance()
    names = f.getPossibleAllocators()
    assert "elastic" in names and names == sorted(names)
    with pytest.raises(aka.Exception, match="already registered"):
        f.registerAllocator("elastic", lambda *a: None)
    name = unique("dup")
    f.registerAllocator(name, lambda *a: None)
    with pytest.raises(aka.Exception, match="already registered"):
        f.registerAllocator(name, lambda *a: None)
    # an allocator must return a Material
    bad = unique("bad")
    f.registerAllocator(bad, lambda dim, opt, model, id_: object())
    with pytest.raises(TypeError, match="must return a Material"):
        law_model(bad)


def test_internal_field_api():
    name = unique("local_elastic_internals")
    mesh, model = bi_material_model(name)
    mat = model.getMaterial(0)
    f = mat.getInternalReal("factor")
    assert isinstance(f, aka.InternalFieldReal)
    assert f.getID().endswith(":factor")
    assert "InternalFieldReal" in repr(f) and "_triangle_3" in repr(f)
    assert f.elementTypes() == [aka._triangle_3]
    assert not f.hasHistory()
    f.initializeHistory()
    assert f.hasHistory()
    prev = f.previous(aka._triangle_3)
    assert prev.shape == f(aka._triangle_3).shape and not prev.flags.writeable
    assert isinstance(f.previous(), aka.InternalFieldReal)
    filt = mat.getElementFilter()(aka._triangle_3)
    assert not filt.flags.writeable and filt.shape[1] == 1
    assert mat.getSpatialDimension() == 2
    # a standalone handler
    h = aka.ConstitutiveLawInternalHandler("h", 2, "parent")
    r = h.registerInternalReal("x", 3)
    assert isinstance(r, aka.InternalFieldReal)
    assert isinstance(h.getInternalReal("x"), aka.InternalFieldReal)
    assert isinstance(h.registerInternalInt("i", 1), aka.InternalFieldInt)
    assert h.getSpatialDimension() == 2
    with pytest.raises(aka.Exception):
        h.getInternalReal("missing")


# --- selectors ---------------------------------------------------------------


TWO_MATERIALS = """
material elastic [
         name    = soft
         rho     = 1.
         nu      = 0.3
         E       = 1.
]
material elastic [
         name    = hard
         rho     = 1.
         nu      = 0.3
         E       = 10.
]
"""


def two_material_model(make_selector=None):
    with open("two.dat", "w") as fh:
        fh.write(TWO_MATERIALS)
    aka.parseInput("two.dat")
    mesh = aka.Mesh(2)
    mesh.read("square.msh")
    model = aka.SolidMechanicsModel(mesh)
    if make_selector is not None:
        model.setMaterialSelector(make_selector(model))
    model.initFull(_analysis_method=aka._static)
    return mesh, model


def test_python_selector_and_fallbacks():
    seen = []

    class ByParity(aka.ConstitutiveLawSelector):
        def __call__(self, element):
            seen.append(element)
            return element.element % 2

    sel = ByParity()
    mesh, model = two_material_model(lambda m: sel)
    assert model.getMaterialSelector() is sel
    idx = model.getMaterialByElement()(aka._triangle_3)[:, 0]
    assert np.array_equal(idx, np.arange(idx.shape[0]) % 2)
    assert seen
    assert model.getMaterial(1).getReal("E") == 10.0

    class Delegate(aka.ConstitutiveLawSelector):
        def __call__(self, element):
            return super().__call__(element)

    d = Delegate()
    d.setFallback(1)
    _, model2 = two_material_model(lambda m: d)
    assert (model2.getMaterialByElement()(aka._triangle_3) == 1).all()


def test_mesh_data_selector_and_default_selector():
    def make(model):
        s = aka.MeshDataMaterialSelectorString("physical_names", model)
        s.setFallback(1)
        return s

    _, model = two_material_model(make)
    sel = model.getMaterialSelector()
    # no law is named "bulk": every element takes the fallback
    assert (model.getMaterialByElement()(aka._triangle_3) == 1).all()
    _, model = two_material_model()
    d = model.getMaterialSelector()
    assert isinstance(d, aka.DefaultMaterialSelector)
    assert (model.getMaterialByElement()(aka._triangle_3) == 0).all()
    with pytest.raises(aka.Exception):
        sel(aka.Element(aka._triangle_3, 0, aka._not_ghost))
