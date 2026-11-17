# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests (group 1): the solver layer of the Python interface
# — python/py_solver.cc (SparseMatrix, SparseMatrixAIJ, SolverVector,
# SparseSolver*, TermToAssemble, TermsToAssemble), python/py_dof_manager.cc
# (DOFManager, NonLinearSolver*, TimeStepSolver, SolverCallback,
# InterceptSolverCallback), python/py_integration_scheme.cc and the
# `AkantuSparseMatrix` of python/akantu/__init__.py — called with upstream's
# argument forms; the InterceptSolverCallback test follows upstream
# examples/python/solid_mechanics_model/solver_callback/solver_callback.py.
"""Solver layer: DOF manager, matrices, terms, schemes, solver callbacks."""

import os

import numpy as np
import pytest

import akantu as aka
import py11_akantu_test_common as tc
from _paths import MESHES, PARSER_FIXTURES

EX = "examples/python/solid_mechanics_model/solver_callback"


@pytest.fixture(autouse=True)
def _inputs(upstream_dir):
    upstream_dir(
        **{
            "mesh_dcb_2d.msh": MESHES
            / "test/test_python_interface/mesh_dcb_2d.msh",
            "elastic.dat": PARSER_FIXTURES
            / "test/test_python_interface/elastic.dat",
            "bar.msh": MESHES / EX / "bar.msh",
            "material.dat": PARSER_FIXTURES / EX / "material.dat",
        }
    )


def eigen_options(method):
    return aka.ModelOptions(
        method,
        aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen),
    )


def static_model():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(eigen_options(aka._static))
    return mesh, model


def bar_model(method=aka._implicit_dynamic):
    aka.parseInput("material.dat")
    mesh = aka.Mesh(2)
    mesh.read("bar.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(eigen_options(method))
    model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")
    return mesh, model


# --- class hierarchy and enums ---------------------------------------------


def test_hierarchy_matches_upstream():
    assert issubclass(aka.ModelSolver, aka.SolverCallback)
    assert issubclass(aka.Model, aka.ModelSolver)
    assert issubclass(aka.SolidMechanicsModel, aka.SolverCallback)
    assert issubclass(aka.InterceptSolverCallback, aka.SolverCallback)
    assert issubclass(aka.SparseMatrixAIJ, aka.SparseMatrix)
    assert issubclass(aka.SparseSolverEigen, aka.SparseSolver)
    assert issubclass(aka.SparseSolver, aka.Parsable)
    assert issubclass(aka.NonLinearSolverNewtonRaphson, aka.NonLinearSolver)
    assert issubclass(aka.NonLinearSolverLumped, aka.NonLinearSolver)
    for cls in (aka.CentralDifference, aka.TrapezoidalRule2, aka.FoxGoodwin,
                aka.LinearAceleration):
        assert issubclass(cls, aka.NewmarkBeta)
    assert issubclass(aka.NewmarkBeta, aka.IntegrationScheme2ndOrder)
    assert issubclass(aka.IntegrationScheme2ndOrder, aka.IntegrationScheme)


def test_solution_type_enum():
    st = aka.IntegrationScheme.SolutionType
    assert int(st._not_defined) == -1
    assert int(st._displacement) == 0 == int(st._temperature)
    assert int(st._damage) == 0
    assert int(st._velocity) == 1 == int(st._temperature_rate)
    assert int(st._acceleration) == 2
    # `.export_values()` into the IntegrationScheme scope
    assert aka.IntegrationScheme._acceleration == st._acceleration
    assert st._velocity == 1
    assert "_displacement" in st.__members__
    assert not hasattr(aka, "SolutionType")


# --- DOF manager and matrices ------------------------------------------------


def test_dof_manager_matrix_k():
    _, model = static_model()
    model.assembleStiffnessMatrix()
    dof = model.getDOFManager()
    assert isinstance(dof, aka.DOFManager)
    assert dof.hasMatrix("K") and dof.hasMatrix(name="K")
    assert not dof.hasMatrix("bogus")
    K = dof.getMatrix("K")
    assert type(K) is aka.SparseMatrixAIJ
    n = 1409 * 2
    assert K.size() == n
    assert K.getMatrixType() == aka._symmetric
    assert K.isFinite()
    irn, jcn, a = K.getIRN(), K.getJCN(), K.getA()
    assert irn.dtype == np.int32 and jcn.dtype == np.int32
    assert irn.shape == jcn.shape == a.shape == (irn.shape[0], 1)
    assert irn.min() >= 1 and irn.max() <= n  # 1-based, as upstream
    assert not a.flags.writeable
    # __call__ reads entries (0-based)
    k = 17
    i, j = int(irn[k, 0]) - 1, int(jcn[k, 0]) - 1
    assert K(i, j) == pytest.approx(a[:, 0][(irn[:, 0] == i + 1)
                                            & (jcn[:, 0] == j + 1)].sum())
    # SciPy wrapper (symmetric → full storage)
    S = aka.AkantuSparseMatrix(K).toarray()
    assert S.shape == (n, n)
    assert np.allclose(S, S.T)
    assert S[i, j] == pytest.approx(K(i, j))
    with pytest.raises(aka.Exception):
        dof.getMatrix("bogus")


def test_matrix_operations_and_releases(tmp_path):
    _, model = static_model()
    model.assembleStiffnessMatrix()
    dof = model.getDOFManager()
    K = dof.getMatrix("K")
    r0 = K.getRelease()
    K2 = dof.getNewMatrix("K2", "K")
    assert dof.hasMatrix("K2")
    assert np.array_equal(K2.getA(), K.getA())
    K2.add(K, 2.0)
    assert np.allclose(K2.getA(), 3.0 * K.getA())
    K2.add(A=K, alpha=-3.0)
    assert np.allclose(K2.getA(), 0.0, atol=1e-6 * abs(K.getA()).max())
    # A matrix added to itself (upstream: K <- (1 + alpha) K)
    K2.add(K, 1.0)
    a2 = K2.getA().copy()
    K2.add(K2, 2.0)
    assert np.array_equal(K2.getA(), a2 + 2.0 * a2)
    K2.zero()
    assert not K2.getA().any()
    i = int(K.getIRN()[0, 0]) - 1
    j = int(K.getJCN()[0, 0]) - 1
    v = K(i, j)
    K.add(i, j, 1.5)
    assert K(i, j) == pytest.approx(v + 1.5)
    assert K.getRelease() != r0
    K.saveMatrix(str(tmp_path / "K.mtx"))
    K.saveProfile(str(tmp_path / "K.prof"))
    assert os.path.getsize(tmp_path / "K.mtx") > 0
    head = open(tmp_path / "K.mtx").readline()
    assert head.startswith("%%MatrixMarket")
    assert os.path.exists(tmp_path / "K.prof")


def test_residual_is_distributed_after_distribute():
    """Upstream's `Mesh.distribute()` marks the mesh
    distributed on one process too, and the DOF manager of a model on a
    distributed mesh has a distributed residual (`SolverVectorDistributed`,
    oracle snippet `residual_distributed`); the port's `isDistributed()`
    answered `False`. Read after `initFull` (the port binds the manager to
    the mesh at its first nodal registration, parity row DOF-013)."""
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    mesh.distribute()
    assert mesh.isDistributed()
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(eigen_options(aka._static))
    res = model.getDOFManager().getResidual()
    assert res.isDistributed()
    # one process: nothing else changes (the solve is the serial one)
    model.solveStep()
    assert res.isDistributed()


def test_residual_and_assembly_to_residual():
    mesh, model = static_model()
    dof = model.getDOFManager()
    res = dof.getResidual()
    assert isinstance(res, aka.SolverVector)
    assert not res.isDistributed()
    dof.zeroResidual()
    v = res.getValues()
    assert v.shape == (2 * mesh.getNbNodes(), 1) and not v.any()
    f = np.zeros((mesh.getNbNodes(), 2))
    f[3, 1] = 2.0
    f[7, 0] = -1.0
    dof.assembleToResidual("displacement", f, scale_factor=0.5)
    dof.assembleToResidual("displacement", f)
    out = np.zeros((mesh.getNbNodes(), 2))
    dof.getArrayPerDOFs("displacement", dof.getResidual(), out)
    assert np.allclose(out, 1.5 * f)
    # a fresh handle sees the live vector
    assert np.allclose(np.sort(dof.getResidual().getValues()[:, 0]),
                       np.sort((1.5 * f).ravel()))
    # `global_vector` is read, also another model's
    _, other = static_model()
    odof = other.getDOFManager()
    odof.zeroResidual()
    odof.assembleToResidual("displacement", -4.0 * f)
    out2 = np.zeros((mesh.getNbNodes(), 2))
    dof.getArrayPerDOFs("displacement", odof.getResidual(), out2)
    # a copy of an assembled (0 + x) residual: no rounding anywhere
    assert np.array_equal(out2, -4.0 * f)


def test_terms_to_assemble():
    t = aka.TermToAssemble(3, 4)
    assert (t.i, t.j) == (3, 4)
    t += 2.0
    t += 0.5
    assert float(t) == 2.5
    terms = aka.TermsToAssemble("displacement", "displacement")
    assert terms.getDOFIdM() == "displacement"
    assert terms.getDOFIdN() == "displacement"
    term = terms(10, 10, 3.0)
    assert (term.i, term.j) == (10, 10)
    term += 1.0  # a copy (UD-831, as upstream): the container keeps 3.0
    assert float(term) == 4.0
    terms(i=11, j=11)  # val = 0.
    assert len(terms) == 2
    _, model = static_model()
    model.assembleStiffnessMatrix()
    dof = model.getDOFManager()
    K = dof.getMatrix("K")
    k10 = K(10, 10)
    dof.assemblePreassembledMatrix("K", terms)
    assert K(10, 10) == pytest.approx(k10 + 3.0)
    dof.assemblePreassembledMatrix(matrix_id="K", terms=terms)
    assert K(10, 10) == pytest.approx(k10 + 6.0)


def test_assemble_to_lumped_matrix():
    mesh, model = bar_model(aka._explicit_lumped_mass)
    model.assembleMassLumped()
    dof = model.getDOFManager()
    ones = np.ones((mesh.getNbNodes(), 2))
    dof.assembleToLumpedMatrix("displacement", ones, "M", scale_factor=2.0)
    dof.assembleToLumpedMatrix(dof_id="displacement", array_to_assemble=ones,
                               lumped_mtx="M")
    with pytest.raises(aka.Exception):
        dof.assembleToLumpedMatrix("displacement", ones, "bogus")


# --- model solver objects -----------------------------------------------------


def test_non_linear_solver_classes():
    _, model = static_model()
    nls = model.getNonLinearSolver()
    assert type(nls) is aka.NonLinearSolverNewtonRaphson
    ss = nls.getSparseSolver()
    assert type(ss) is aka.SparseSolverEigen
    nls.set("max_iterations", 3.7)
    assert nls.getInt("max_iterations") == 3
    nls.set("convergence_type", aka.SolveConvergenceCriteria.solution)
    _, explicit = bar_model(aka._explicit_lumped_mass)
    assert type(explicit.getNonLinearSolver()) is aka.NonLinearSolverLumped


def test_time_step_solver_and_schemes():
    _, model = bar_model(aka._explicit_lumped_mass)
    tss = model.getTimeStepSolver()
    assert isinstance(tss, aka.TimeStepSolver)
    cd = tss.getIntegrationScheme("displacement")
    assert type(cd) is aka.CentralDifference
    assert (cd.alpha, cd.beta) == (0.0, 0.5)
    _, implicit = bar_model(aka._implicit_dynamic)
    tr = implicit.getTimeStepSolver().getIntegrationScheme("displacement")
    assert type(tr) is aka.TrapezoidalRule2
    assert (tr.alpha, tr.beta) == (0.5, 0.5)
    tr.beta = 0.6
    again = implicit.getTimeStepSolver("").getIntegrationScheme("displacement")
    assert again.beta == 0.6
    nb = aka.NewmarkBeta(implicit.getDOFManager(), "displacement", 0.25, 0.5)
    assert (nb.alpha, nb.beta) == (0.25, 0.5)
    nb.alpha = 0.3
    assert nb.alpha == 0.3


def test_set_integration_scheme_with_solution_type():
    _, model = bar_model(aka._implicit_dynamic)
    model.getNewSolver(
        "second",
        aka.ModelSolverOptions(
            time_step_solver_type=aka.TimeStepSolverType.dynamic,
            sparse_solver_type=aka.SparseSolverType.eigen,
        ),
    )
    model.setIntegrationScheme(
        "second", "displacement", aka.IntegrationSchemeType.newmark_beta,
        solution_type=aka.IntegrationScheme._acceleration)
    nb = model.getTimeStepSolver("second").getIntegrationScheme(
        "displacement")
    assert type(nb) is aka.NewmarkBeta


def test_model_solver_callback_interface():
    _, model = static_model()
    assert model.getMatrixType("K") == aka._symmetric
    model.assembleMatrix("K")
    assert model.getDOFManager().getMatrix("K").getRelease() > 0
    model.assembleResidual()
    model.predictor()
    model.corrector()
    model.beforeSolveStep()
    model.afterSolveStep(True)
    assert isinstance(model.dof_manager, aka.DOFManager)


# --- solver callbacks --------------------------------------------------------


class Periodic(aka.InterceptSolverCallback):
    """Upstream solver_callback.py's callback (left/right nodes tied)."""

    def __init__(self, model):
        super().__init__(model)
        self.model = model
        mesh = model.getMesh()
        left = mesh.getElementGroup("Left").getNodeGroup().getNodes()
        right = mesh.getElementGroup("Right").getNodeGroup().getNodes()
        position = mesh.getNodes()
        self.pair = []
        # upstream: `int(node_l)` on the (n, 1) rows (an error since NumPy 2.5)
        for node_l in left:
            node_l = int(np.ravel(node_l)[0])
            for node_r in right:
                node_r = int(np.ravel(node_r)[0])
                if abs(position[node_r, 1] - position[node_l, 1]) < 1e-6:
                    self.pair.append([node_l, node_r])
        self.periodic_K_modif = aka.TermsToAssemble(
            "displacement", "displacement")
        matrix_type = self.model.getMatrixType("K")
        for p in self.pair:
            self.periodic_K_modif(p[0] * 2, p[0] * 2, 1)
            self.periodic_K_modif(p[0] * 2, p[1] * 2, -1)
            if matrix_type == aka._unsymmetric:
                self.periodic_K_modif(p[1] * 2, p[0] * 2, -1)
                self.periodic_K_modif(p[1] * 2, p[1] * 2, 1)
        self.first = True
        self.k_release = -1
        self.calls = {"K": 0, "r": 0}

    def assembleMatrix(self, matrix_id):
        self.model.assembleMatrix(matrix_id)
        if matrix_id == "K":
            self.calls["K"] += 1
            release = self.model.getDOFManager().getMatrix("K").getRelease()
            if release == self.k_release:
                return
            self.model.getDOFManager().assemblePreassembledMatrix(
                "K", self.periodic_K_modif)
            self.k_release = (
                self.model.getDOFManager().getMatrix("K").getRelease())
            self.first = False

    def assembleResidual(self):
        self.calls["r"] += 1
        displacement = self.model.getDisplacement()
        force = np.zeros(displacement.shape)
        for p in self.pair:
            force[p[0], 0] += displacement[p[0], 0] - displacement[p[1], 0]
            force[p[1], 0] += displacement[p[1], 0] - displacement[p[0], 0]
        self.model.getDOFManager().assembleToResidual(
            "displacement", force, -1.0)
        self.model.assembleResidual()


def wave(model, mesh):
    displacement = model.getDisplacement()
    velocity = model.getVelocity()
    position = mesh.getNodes()
    A, L = 0.01, 1
    v = np.sqrt(model.getMaterial(0).getReal("E")
                / model.getMaterial(0).getReal("rho"))
    k = 0.1 * 2 * np.pi * 3 / L
    velocity[:, 0] = k * v * A * np.sin(k * (position[:, 0] - 5.0))
    displacement[:, 0] = A * np.cos(k * (position[:, 0] - 5.0))
    model.setTimeStep(model.getStableTimeStep() * 0.64)


def test_intercept_solver_callback_runs_upstream_example_logic():
    mesh, model = bar_model()
    wave(model, mesh)
    cb = Periodic(model)
    assert cb.pair, "left/right node pairs"
    solver = model.getNonLinearSolver()
    solver.set("max_iterations", 100)
    solver.set("threshold", 1e-7)
    for _ in range(3):
        model.solveStep(cb)
    assert cb.calls["K"] >= 1 and cb.calls["r"] >= 3
    # the periodic terms were added once per K release
    assert not cb.first
    u = model.getDisplacement()
    assert np.isfinite(u).all()
    # the tie pulls the paired x displacements towards each other
    pair = np.array(cb.pair)
    gap = np.abs(u[pair[:, 0], 0] - u[pair[:, 1], 0]).max()
    assert gap < 0.02
    epot, ekin = model.getEnergy("potential"), model.getEnergy("kinetic")
    assert np.isfinite(epot) and np.isfinite(ekin)


def test_intercept_without_overrides_equals_plain_solve():
    mesh, a = bar_model()
    wave(a, mesh)
    mesh_b, b = bar_model()
    wave(b, mesh_b)

    class Nothing(aka.InterceptSolverCallback):
        pass

    for _ in range(2):
        a.solveStep()
        b.solveStep(Nothing(b))
        b.solveStep(callback=Nothing(b), solver_id="")
        a.solveStep("")
    assert np.array_equal(a.getDisplacement(), b.getDisplacement())


def test_intercept_super_calls_and_counts():
    mesh, model = bar_model()
    wave(model, mesh)
    seen = []

    class Spy(aka.InterceptSolverCallback):
        def predictor(self):
            seen.append("predictor")
            super().predictor()

        def corrector(self):
            seen.append("corrector")
            super().corrector()

        def beforeSolveStep(self):
            seen.append("before")
            super().beforeSolveStep()

        def afterSolveStep(self, converged):
            seen.append(("after", converged))
            super().afterSolveStep(converged)

        def assembleMatrix(self, matrix_id):
            seen.append(("K", matrix_id))
            super().assembleMatrix(matrix_id)

        def getMatrixType(self, matrix_id):
            seen.append(("type", matrix_id))
            return super().getMatrixType(matrix_id)

    mesh_b, ref = bar_model()
    wave(ref, mesh_b)
    model.solveStep(Spy(model))
    ref.solveStep()
    assert seen[0] == "before" and seen[-1] == ("after", True)
    assert "predictor" in seen and "corrector" in seen
    assert np.array_equal(model.getDisplacement(), ref.getDisplacement())


def test_callback_exception_keeps_its_type():
    mesh, model = bar_model()
    wave(model, mesh)

    class Boom(aka.InterceptSolverCallback):
        def assembleResidual(self):
            raise KeyError("from the callback")

    with pytest.raises(KeyError, match="from the callback"):
        model.solveStep(Boom(model))
    # the model is usable afterwards
    model.solveStep()


def test_user_dof_manager_is_refused_like_upstream():
    """D-078: upstream's binding raises
    ``RuntimeError: Unable to cast from non-held to held instance`` for a
    DOF manager passed to a model constructor (the only kind Python can
    obtain is a model's); the port raises the same type. Anything else is
    an incompatible argument; ``None`` is the default."""
    mesh, model = static_model()
    dm = model.getDOFManager()
    for ctor in (lambda: aka.SolidMechanicsModel(mesh, 2, "other", dm),
                 lambda: aka.SolidMechanicsModel(mesh, 2, "other2",
                                                 dof_manager=dm),
                 lambda: aka.PhaseFieldModel(mesh, 2, "pf", dm),
                 lambda: aka.ContactMechanicsModel(mesh, 2, "cm", dm),
                 lambda: aka.CouplerSolidContact(mesh, 2, "sc", dm)):
        with pytest.raises(RuntimeError, match="non-held to held instance"):
            ctor()
    with pytest.raises(TypeError):
        aka.SolidMechanicsModel(mesh, 2, "other3", 7)
    aka.SolidMechanicsModel(mesh, 2, "other4", None)


INTERCEPT_METHODS = ["assembleMatrix", "assembleLumpedMatrix",
                     "assembleResidual", "predictor", "corrector",
                     "beforeSolveStep", "afterSolveStep"]


@pytest.mark.parametrize("name", INTERCEPT_METHODS)
def test_every_intercepted_method_carries_its_exception(name):
    """Program A3: a Python exception raised in an
    overridden ``InterceptSolverCallback`` method reaches ``solveStep`` with
    its own type and message, as upstream (identical on the oracle); the
    model solves natively afterwards."""
    method = (aka._explicit_lumped_mass if name == "assembleLumpedMatrix"
              else aka._implicit_dynamic)
    mesh, model = bar_model(method)
    wave(model, mesh)
    exc = type("E_" + name, (Exception,), {})

    def raiser(self, *args):
        raise exc("from " + name)

    cls = type("Raise_" + name, (aka.InterceptSolverCallback,), {name: raiser})
    with pytest.raises(exc, match="^from " + name + "$"):
        model.solveStep(cls(model))
    model.solveStep()
    assert np.isfinite(model.getDisplacement()).all()


def test_get_matrix_type_exception_is_carried_out_of_native_code():
    """``getMatrixType`` is infallible in native code;
    an exception of a Python override is carried and re-raised with its
    type (``_test_common.callbackMatrixType`` asks the trampoline as the
    solver does when it sets an integration scheme). A solve itself does not
    ask the user callback for matrix types — upstream and port query them at
    ``setIntegrationScheme`` with the model as the callback — so the A3
    case ``x.getMatrixType`` completes on both modules (oracle: no
    exception), as here."""
    mesh, model = bar_model()
    wave(model, mesh)

    class E(Exception):
        pass

    class Intercept(aka.InterceptSolverCallback):
        def getMatrixType(self, matrix_id):
            raise E("type of " + matrix_id)

    class Plain(aka.SolverCallback):
        def __init__(self, model):
            super().__init__(model.getDOFManager())

        def getMatrixType(self, matrix_id):
            raise LookupError("plain " + matrix_id)

    class Answer(aka.InterceptSolverCallback):
        def getMatrixType(self, matrix_id):
            return aka._unsymmetric

    with pytest.raises(E, match="^type of K$"):
        tc.callbackMatrixType(model, Intercept(model), "K")
    with pytest.raises(LookupError, match="^plain K$"):
        tc.callbackMatrixType(model, Plain(model), "K")
    assert tc.callbackMatrixType(model, Answer(model), "K") == aka._unsymmetric
    model.solveStep(Intercept(model))  # not asked during the solve
    assert np.isfinite(model.getDisplacement()).all()


def test_model_calls_refused_outside_the_running_solve_access():
    mesh, model = bar_model()
    wave(model, mesh)
    stored = {}

    class Keep(aka.InterceptSolverCallback):
        def assembleResidual(self):
            stored["dof"] = self.model.getDOFManager()
            self.model.assembleResidual()
            # another model method that is not part of the solve access
            with pytest.raises(aka.Exception, match="in use"):
                self.model.getStableTimeStep()

    cb = Keep(model)
    cb.model = model
    model.solveStep(cb)
    # after the solve, a stored DOF manager works on the model again
    assert stored["dof"].hasMatrix("K")


def test_plain_solver_callback_replaces_the_model():
    _, ref = static_model()
    _, model = static_model()
    for m in (ref, model):
        blocked = m.getBlockedDOFs()
        X = m.getMesh().getNodes()
        blocked[X[:, 0] < 1e-12, :] = True
        right = X[:, 0] > X[:, 0].max() - 1e-12
        m.getExternalForce()[right, 1] = -10.0

    class Linear(aka.SolverCallback):
        def __init__(self, model):
            super().__init__(model.getDOFManager())
            self.model = model
            self.calls = []

        def getMatrixType(self, matrix_id):
            self.calls.append("type")
            return self.model.getMatrixType(matrix_id)

        def assembleMatrix(self, matrix_id):
            self.calls.append("K")
            self.model.assembleMatrix(matrix_id)

        def assembleLumpedMatrix(self, matrix_id):
            self.model.assembleLumpedMatrix(matrix_id)

        def assembleResidual(self):
            self.calls.append("r")
            self.model.assembleResidual()

    cb = Linear(model)
    assert isinstance(cb.dof_manager, aka.DOFManager)
    ref.solveStep()
    model.solveStep(cb)
    assert "K" in cb.calls and "r" in cb.calls
    assert np.array_equal(model.getDisplacement(), ref.getDisplacement())


def test_plain_solver_callback_pure_virtual_methods():
    _, model = static_model()

    class Incomplete(aka.SolverCallback):
        def __init__(self, model):
            super().__init__(model.getDOFManager())

        def getMatrixType(self, matrix_id):
            return aka._symmetric

        def assembleMatrix(self, matrix_id):
            pass

    with pytest.raises(Exception, match="pure virtual"):
        model.solveStep(Incomplete(model))
    plain = aka.SolverCallback(model.getDOFManager())
    with pytest.raises(RuntimeError, match="pure virtual"):
        plain.assembleResidual()
    plain.predictor()  # base no-op


def _dcb_static(release):
    """Two static solves of the DCB mesh (left edge clamped, the second
    solve with twice the load); `release`: releaseStaticSolver() between
    them (port extension)."""
    mesh, model = static_model()
    x = mesh.getNodes()
    blocked = model.getBlockedDOFs()
    blocked[:] = False
    blocked[np.isclose(x[:, 0], x[:, 0].min()), :] = True
    f = model.getExternalForce()
    f[:] = 0
    f[np.isclose(x[:, 0], x[:, 0].max()), 1] = 1.0
    model.solveStep()
    nnz = None
    if release:
        model.releaseStaticSolver()
        dof = model.getDOFManager()
        nnz = (dof.getMatrix("K").getA().shape[0],
               dof.getMatrix("J").getA().shape[0])
    f *= 2.0
    model.solveStep()
    return model.getDisplacement().copy(), nnz


def test_release_static_solver_then_resolve():
    kept, _ = _dcb_static(False)
    released, nnz = _dcb_static(True)
    assert nnz == (0, 0)
    assert np.abs(kept).max() > 0
    # the second solve assembles and factorises again: the same bits
    assert np.array_equal(kept.view(np.uint64), released.view(np.uint64))


def _dcb_static_ordering(ordering):
    """One static solve of the DCB mesh (left edge clamped, unit load on
    the right edge) with the eigen solver's `ordering` (None: default)."""
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    kw = {} if ordering is None else {"sparse_solver_ordering": ordering}
    model.initFull(aka.ModelOptions(aka._static, aka.ModelSolverOptions(
        sparse_solver_type=aka.SparseSolverType.eigen, **kw)))
    x = mesh.getNodes()
    blocked = model.getBlockedDOFs()
    blocked[:] = False
    blocked[np.isclose(x[:, 0], x[:, 0].min()), :] = True
    f = model.getExternalForce()
    f[:] = 0
    f[np.isclose(x[:, 0], x[:, 0].max()), 1] = 1.0
    model.solveStep()
    return model.getDisplacement().copy()


def test_sparse_solver_ordering_option():
    """akantu-rs extension (D-125): ModelSolverOptions(sparse_solver_ordering=)
    selects the eigen solver's column ordering; `colamd` is the default's
    bits, nested dissection agrees in the last digits; unknown names are
    refused."""
    with pytest.raises(ValueError):
        aka.ModelSolverOptions(sparse_solver_ordering="metis")
    default = _dcb_static_ordering(None)
    np.testing.assert_array_equal(_dcb_static_ordering("colamd"), default)
    nd = _dcb_static_ordering("nested_dissection")
    scale = np.abs(default).max()
    assert np.abs(nd - default).max() <= 1e-10 * scale
