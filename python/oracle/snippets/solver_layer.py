# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (group 1) oracle snippet: the solver layer
# (python/py_solver.cc, py_dof_manager.cc, py_integration_scheme.cc,
# AkantuSparseMatrix of python/akantu/__init__.py) and an
# InterceptSolverCallback following upstream
# examples/python/solid_mechanics_model/solver_callback/solver_callback.py.
# The sparse solver is pinned to Eigen (the oracle's auto solver is MUMPS).
import numpy as np

import akantu as aka
from _out import out


def opts(method):
    return aka.ModelOptions(
        method,
        aka.ModelSolverOptions(sparse_solver_type=aka.SparseSolverType.eigen),
    )


# --- static DCB: K, terms, residual -------------------------------------------
aka.parseInput("elastic.dat")
mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(opts(aka._static))
nls = model.getNonLinearSolver()
out("nls.class", type(nls).__name__)
out("sparse.class", type(nls.getSparseSolver()).__name__)
model.assembleStiffnessMatrix()
dof = model.getDOFManager()
K = dof.getMatrix("K")
out("K.class", type(K).__name__)
out("K.size", K.size())
out("K.type", str(K.getMatrixType()))
out("K.finite", K.isFinite())
irn, jcn, a = K.getIRN(), K.getJCN(), K.getA()
out("K.irn", irn)
out("K.jcn", jcn)
out("K.a", a)
out("K(10,10)", K(10, 10))
out("K(10,11)", K(10, 11))
S = aka.AkantuSparseMatrix(K).tocsr()
out("S.rowsum", np.asarray(S.sum(axis=1)).ravel())
out("hasMatrix", [dof.hasMatrix("K"), dof.hasMatrix("bogus")])
r0 = K.getRelease()
terms = aka.TermsToAssemble("displacement", "displacement")
t = terms(10, 10, 3.0)
t += 1.0  # a copy upstream (UD-831): the container keeps 3.0
terms(10, 12, -2.0)
out("terms.ids", [terms.getDOFIdM(), terms.getDOFIdN(), t.i, t.j])
dof.assemblePreassembledMatrix("K", terms)
out("K(10,10).terms", K(10, 10))
out("K(10,12).terms", K(10, 12))
out("K.release.changed", K.getRelease() != r0)
K2 = dof.getNewMatrix("K2", "K")
# the copy's coordinates and values; lookups K2(i, j) and later adds differ:
# upstream's copy constructor loses the lookup map (UD-179, corrected)
out("K2.irn", K2.getIRN())
out("K2.a", K2.getA())
out("K2.type", str(K2.getMatrixType()))
X = mesh.getNodes()
blocked = model.getBlockedDOFs()
blocked[X[:, 0] < 1e-12, :] = True
right = X[:, 0] > X[:, 0].max() - 1e-12
model.getExternalForce()[right, 1] = -10.0
model.assembleResidual()
res = dof.getResidual()
out("res.distributed", res.isDistributed())
out("res.values", res.getValues())
per = np.zeros((mesh.getNbNodes(), 2))
dof.getArrayPerDOFs("displacement", res, per)
out("res.perdof", per)
dof.zeroResidual()
f = np.zeros((mesh.getNbNodes(), 2))
f[3, 1], f[7, 0] = 2.0, -1.0
dof.assembleToResidual("displacement", f, 0.5)
out("res.assembled", dof.getResidual().getValues())
out("matrixType(K)", str(model.getMatrixType("K")))

# --- schemes ---------------------------------------------------------------------
aka.parseInput("material.dat")
bar = aka.Mesh(2)
bar.read("bar.msh")
explicit = aka.SolidMechanicsModel(bar)
explicit.initFull(opts(aka._explicit_lumped_mass))
out("explicit.nls", type(explicit.getNonLinearSolver()).__name__)
try:
    # upstream raises RuntimeError ("return_value_policy = copy, but type
    # is non-copyable", UD-830): the port's extra key is not compared
    cd = explicit.getTimeStepSolver().getIntegrationScheme("displacement")
    out("cd", [type(cd).__name__, cd.alpha, cd.beta])
except RuntimeError:
    pass
st = aka.IntegrationScheme.SolutionType
out("solution_types", [int(st._not_defined), int(st._displacement),
                       int(st._velocity), int(st._acceleration),
                       int(aka.IntegrationScheme._temperature_rate)])

bar2 = aka.Mesh(2)
bar2.read("bar.msh")
model = aka.SolidMechanicsModel(bar2)
model.initFull(opts(aka._implicit_dynamic))
try:
    tr = model.getTimeStepSolver().getIntegrationScheme("displacement")
    out("tr", [type(tr).__name__, tr.alpha, tr.beta])
except RuntimeError:
    pass
nb = aka.NewmarkBeta(model.getDOFManager(), "displacement", 0.25, 0.5)
out("nb", [nb.alpha, nb.beta])
model.applyBC(aka.FixedValue(0, aka._y), "YBlocked")


# --- InterceptSolverCallback (solver_callback.py) -------------------------------
class SolverCallback(aka.InterceptSolverCallback):
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
        self.k_release = -1
        self.nb_k = 0

    def assembleMatrix(self, matrix_id):
        self.model.assembleMatrix(matrix_id)
        if matrix_id == "K":
            release = self.model.getDOFManager().getMatrix("K").getRelease()
            if release == self.k_release:
                return
            self.nb_k += 1
            self.model.getDOFManager().assemblePreassembledMatrix(
                "K", self.periodic_K_modif)
            self.k_release = (
                self.model.getDOFManager().getMatrix("K").getRelease())

    def assembleResidual(self):
        displacement = self.model.getDisplacement()
        force = np.zeros(displacement.shape)
        for p in self.pair:
            force[p[0], 0] += displacement[p[0], 0] - displacement[p[1], 0]
            force[p[1], 0] += displacement[p[1], 0] - displacement[p[0], 0]
        self.model.getDOFManager().assembleToResidual(
            "displacement", force, -1.0)
        self.model.assembleResidual()


displacement = model.getDisplacement()
velocity = model.getVelocity()
position = bar2.getNodes()
A, L = 0.01, 1
v = np.sqrt(model.getMaterial(0).getReal("E")
            / model.getMaterial(0).getReal("rho"))
k = 0.1 * 2 * np.pi * 3 / L
velocity[:, 0] = k * v * A * np.sin(k * (position[:, 0] - 5.0))
displacement[:, 0] = A * np.cos(k * (position[:, 0] - 5.0))
model.setTimeStep(model.getStableTimeStep() * 0.64)
cb = SolverCallback(model)
out("cb.pairs", len(cb.pair))
solver = model.getNonLinearSolver()
solver.set("max_iterations", 100)
solver.set("threshold", 1e-7)
for step in range(5):
    model.solveStep(cb)
    out(f"step{step}.epot", model.getEnergy("potential"))
    out(f"step{step}.ekin", model.getEnergy("kinetic"))
    out(f"step{step}.niter", solver.getInt("nb_iterations"))
out("cb.nb_k", cb.nb_k)
out("u", model.getDisplacement())
out("v", model.getVelocity())
