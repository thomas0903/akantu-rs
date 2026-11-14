# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
# port): small SolidMechanicsModel runs through the Akantu Python bindings
# (python/py_solid_mechanics_model.cc), recording intermediate state.
#
# Cases (fixtures/model/<kind>/<name>/):
#   static/patch_*        linear elastic static solve with an affine Dirichlet
#                         field (patch test): u, f_ext, f_int, residual
#                         f_ext + f_int, grad_u (= A, asymmetric), stresses,
#                         bit-exact stiffness matrix triplets, solver
#                         iterations/error, measured conditioning
#   static/loaded_*       Neumann-loaded equilibrium (non-zero f_ext): the
#                         residual f_ext + f_int must vanish on free DOFs
#   explicit/...          central-difference steps: u, v, a, f_int, energies
#   implicit_dynamic/...  Newmark (trapezoidal) steps with Akantu's own Newton
#                         residual history (ragged), the pre-inertia part
#                         f_ext + f_int, the consistent mass matrix and the
#                         measured conditioning of the Jacobian
#   layout/...            tensor layout sentinel: distinct values written into
#                         the stress buffer through the bindings and read back
#                         by record_stress
#
# Tensor layout (docs/inventory/core.md errata E7): the bindings expose an
# Array<Real> as a [tuple, component] C-order numpy array; inside a tuple a
# dim x dim tensor is stored column-major (component r + c*dim is M(r, c),
# as make_view(array, dim, dim) reads it in C++). Fixtures store tensors as
# [..., row, col] row-major, so each tuple is transposed (per_quad_tensor).
#
# Residual sign: SolidMechanicsModel::assembleResidual
# assembles external_force and internal_force with coefficient +1 each
# (solid_mechanics_model.cc:245-257); Akantu's internal force already carries
# its sign, so the static residual is f_ext + f_int. The integration scheme
# then adds -M a (integration_scheme_2nd_order.cc:84-90).
#
# Usage (inside akantu-oracle:dev, Akantu Python module on PYTHONPATH):
#   python3 -I model_probe.py <fixtures_root> <akantu_src> [case ...]
from __future__ import annotations

import math
import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import numpy as np  # noqa: E402

import akantu as aka  # noqa: E402
from akantu_probe import FixtureWriter  # noqa: E402

PROBE = "model_probe"
# 2: stress transposed to [row, col], residual f_ext + f_int,
#    bit-exact matrices, grad_u, conditioning, loaded and layout cases.
# 3: parameters.solver records the non-linear solver type (the
#    harness checks the port's stopping decision with that solver's rule).
VERSION = "3"

ELASTIC = """material elastic [
  name = elastic
  rho = 7800.
  E = 2.1e11
  nu = 0.3
]
"""

# Low yield stress so that the implicit dynamic steps plastify and Newton
# needs several iterations (neohookean is non-functional at the reference
# commit: inventory MAT-025, confirmed by fixtures/material/neohookean/*).
PLASTIC = """material plastic_linear_isotropic_hardening [
  name = plastic
  rho = 7800.
  E = 2.1e11
  nu = 0.3
  sigma_y = 1e6
  h = 1e9
]
"""

PATCH = "test/test_model/patch_tests/data/{}.msh"

CASES = {
    "static/patch_triangle_3": dict(kind="static", mesh=PATCH.format("_triangle_3"), dim=2),
    "static/patch_quadrangle_8": dict(kind="static", mesh=PATCH.format("_quadrangle_8"), dim=2),
    "static/patch_tetrahedron_4": dict(kind="static", mesh=PATCH.format("_tetrahedron_4"), dim=3),
    "static/patch_hexahedron_20": dict(kind="static", mesh=PATCH.format("_hexahedron_20"), dim=3),
    "explicit/patch_triangle_3": dict(kind="explicit", mesh=PATCH.format("_triangle_3"), dim=2, steps=20),
    "explicit/patch_hexahedron_8": dict(kind="explicit", mesh=PATCH.format("_hexahedron_8"), dim=3, steps=10),
    "implicit_dynamic/patch_triangle_6_plastic": dict(kind="implicit_dynamic", mesh=PATCH.format("_triangle_6"), dim=2,
                                              steps=5, material=PLASTIC),
    "static/loaded_quadrangle_4": dict(kind="loaded", mesh=PATCH.format("_quadrangle_4"), dim=2),
    "static/loaded_tetrahedron_4": dict(kind="loaded", mesh=PATCH.format("_tetrahedron_4"), dim=3),
    "layout/stress_sentinel_quadrangle_8": dict(kind="layout", mesh=PATCH.format("_quadrangle_8"), dim=2),
    "layout/stress_sentinel_hexahedron_8": dict(kind="layout", mesh=PATCH.format("_hexahedron_8"), dim=3),
}

# Nodal load of the loaded cases (per node of the x = xmax face).
LOAD3 = np.array([2.0e5, -1.0e5, 5.0e4])

# Affine field u = A X + b used for Dirichlet data / initial conditions.
A3 = np.array([[1.0e-3, 2.0e-4, -1.5e-4], [-3.0e-4, 5.0e-4, 1.0e-4], [2.5e-4, -1.0e-4, 8.0e-4]])
B3 = np.array([1.0e-4, -2.0e-4, 3.0e-4])


def element_types(mesh, dim: int):
    """Regular element types of dimension `dim` (the Python Mesh binding
    exposes elementTypes on its connectivity ElementTypeMapArray)."""
    return mesh.getConnectivities().elementTypes(dim=dim, ghost_type=aka._not_ghost,
                                                 kind=aka._ek_regular)


def tname(t) -> str:
    """Akantu element type name ('_triangle_3') of a pybind11 enum value."""
    return getattr(t, "name", str(t).split(".")[-1])


def boundary_nodes(X: np.ndarray) -> np.ndarray:
    lo, hi = X.min(axis=0), X.max(axis=0)
    tol = 1e-10 * max(1.0, float(np.abs(X).max()))
    on = np.zeros(X.shape[0], dtype=bool)
    for d in range(X.shape[1]):
        on |= np.abs(X[:, d] - lo[d]) < tol
        on |= np.abs(X[:, d] - hi[d]) < tol
    return on


def aij(matrix):
    """Bit-exact triplets of an Akantu SparseMatrixAIJ (0-based) and whether
    only one triangle is stored. Unlike SparseMatrix::saveMatrix, which
    prints digits10 = 15 significant digits, this keeps every bit."""
    rows = np.array(matrix.getIRN(), dtype=np.int64).ravel() - 1
    cols = np.array(matrix.getJCN(), dtype=np.int64).ravel() - 1
    vals = np.array(matrix.getA(), dtype=np.float64).ravel()
    symmetric = str(matrix.getMatrixType()).endswith("_symmetric")
    return rows, cols, vals, int(matrix.size()), symmetric


def dense(rows, cols, vals, n, symmetric) -> np.ndarray:
    """Dense matrix of AIJ triplets (duplicates summed, symmetric storage
    mirrored). Only used for measurements (conditioning, checks)."""
    a = np.zeros((n, n))
    np.add.at(a, (rows, cols), vals)
    if symmetric:
        off = rows != cols
        np.add.at(a, (cols[off], rows[off]), vals[off])
    return a


def record_matrix(w: FixtureWriter, name: str, matrix, description: str) -> tuple:
    r, c, v, n, sym = aij(matrix)
    w.parameters[name] = {"shape": [n, n], "symmetric_storage": sym,
                          "dof_numbering": "DOFManager equation numbering, node*dim+component in serial",
                          "source": "SparseMatrixAIJ getIRN/getJCN/getA (bit-exact, 0-based)"}
    w.add(f"{name}/rows", r, ["nnz"], "matrix", tolerance_class="exact")
    w.add(f"{name}/cols", c, ["nnz"], "matrix", tolerance_class="exact")
    w.add(f"{name}/values", v, ["nnz"], "matrix", tolerance_class="kernel", description=description)
    return r, c, v, n, sym


def cond2(a: np.ndarray) -> float:
    """2-norm condition number (ratio of extreme singular values)."""
    sv = np.linalg.svd(a, compute_uv=False)
    return float(sv[0] / sv[-1])


def sequential_norm(vec: np.ndarray, free: np.ndarray) -> float:
    """sqrt(sum of squares over free DOFs) accumulated in DOF order, exactly
    as NonLinearSolverNewtonRaphson::testConvergence computes it (strict
    build: no FMA contraction), so the result is bit-identical."""
    acc = 0.0
    for x, f in zip(vec.tolist(), free.tolist()):
        if f:
            acc += x * x
    return math.sqrt(acc)


def per_quad_tensor(buffer, nq: int, dim: int) -> np.ndarray:
    """[tuple, dim*dim] binding buffer -> [element, quad, row, col].

    Component r + c*dim of a tuple is M(r, c) (column-major per tuple, as
    make_view reads it); reshaping alone would yield M^T."""
    a = np.array(buffer, dtype=np.float64).reshape(-1, nq, dim, dim)
    return np.ascontiguousarray(a.transpose(0, 1, 3, 2))


def record_mesh(w: FixtureWriter, mesh, dim: int) -> None:
    X = np.array(mesh.getNodes())
    w.add("mesh/nodes", X, ["node", "spatial_dim"], "nodal", tolerance_class="exact",
          description="node coordinates as read")
    for t in element_types(mesh, dim):
        conn = np.array(mesh.getConnectivity(t), dtype=np.int64)
        w.add(f"mesh/connectivity/{tname(t)}", conn, ["element", "node_local"], "elemental",
              element_type=tname(t), tolerance_class="exact")


def record_stress(w: FixtureWriter, model, mesh, dim: int, prefix: str = "",
                  tolerance_class: str = "linear_solve"):
    mat = model.getMaterial(0)
    for t in element_types(mesh, dim):
        nq = model.getFEEngine().getNbIntegrationPoints(t)
        s = per_quad_tensor(mat.getStress(t), nq, dim)
        w.add(f"{prefix}stress/{tname(t)}", s, ["element", "quad", "row", "col"], "quadrature",
              element_type=tname(t), tolerance_class=tolerance_class,
              description="material 0 stress sigma(row, col) per integration point "
                          "(element order of its element filter)")


def record_grad_u(w: FixtureWriter, model, mesh, dim: int):
    mat = model.getMaterial(0)
    for t in element_types(mesh, dim):
        nq = model.getFEEngine().getNbIntegrationPoints(t)
        g = per_quad_tensor(mat.getGradU(t), nq, dim)
        w.add(f"grad_u/{tname(t)}", g, ["element", "quad", "row", "col"], "quadrature",
              element_type=tname(t), tolerance_class="linear_solve",
              description="material 0 displacement gradient du_row/dX_col per integration "
                          "point (asymmetric for these cases: checks the tensor layout)")


def record_forces(w: FixtureWriter, model, blocked) -> dict:
    """f_ext, f_int and the static residual f_ext + f_int; returns norms."""
    fext = np.array(model.getExternalForce())
    fint = np.array(model.getInternalForce())
    res = fext + fint
    free = ~np.array(blocked, dtype=bool)
    w.add("external_force", fext, ["node", "component"], "nodal", tolerance_class="exact")
    w.add("internal_force", fint, ["node", "component"], "nodal", tolerance_class="linear_solve")
    w.add("residual", res, ["node", "component"], "nodal", tolerance_class="linear_solve",
          description="static residual f_ext + f_int as assembled by "
                      "SolidMechanicsModel::assembleResidual (both coefficients +1); "
                      "~0 on free DOFs at equilibrium, reactions on blocked DOFs")
    return {"residual_norm_free": sequential_norm(res.ravel(), free.ravel()),
            "external_force_norm_free": sequential_norm(fext.ravel(), free.ravel())}


def measure_static_conditioning(w: FixtureWriter, krec: tuple, blocked, disp, fext, norms: dict):
    """Record cond2(K_ff) and the backward error of the constrained solve."""
    r, c, v, n, sym = krec
    k = dense(r, c, v, n, sym)
    free = ~np.array(blocked, dtype=bool).ravel()
    u = np.array(disp).ravel()
    kff = k[np.ix_(free, free)]
    rhs = np.array(fext).ravel()[free] - k[np.ix_(free, ~free)] @ u[~free]
    res = kff @ u[free] - rhs
    knorm = float(np.linalg.norm(kff, 2))
    w.parameters["conditioning"] = {
        "cond2_K_free": cond2(kff),
        "free_dofs": int(free.sum()),
        "norm2_K_free": knorm,
        "backward_error": float(np.linalg.norm(res) /
                                (knorm * np.linalg.norm(u[free]) + np.linalg.norm(rhs))),
        "residual_norm_free": norms["residual_norm_free"],
        "method": "numpy SVD / 2-norms of K[free, free] assembled from the bit-exact "
                  "AIJ triplets; backward error |K_ff u_f - (f_f - K_fb u_b)| / "
                  "(|K_ff| |u_f| + |rhs|) in the 2-norm (probe-side numpy arithmetic)",
    }


def solver_settings(solver, max_iterations: int, threshold: float, convergence_type: str,
                    **extra) -> dict:
    """`parameters.solver`: the settings applied by the probe plus the
    non-linear solver type, whose stopping rule the harness applies to the
    port's results (akantu_validation::solver). The `type` parameter is
    parsable-only, not readable through the bindings: it is the
    SolidMechanicsModel default of the analysis method (_newton_raphson for
    _static and implicit _dynamic, solid_mechanics_model.cc:146-160; the
    probe does not override it). The class returned by the bindings is
    checked (NonLinearSolverNewtonRaphson::solve, whose `this->linear`
    variant is only created for an explicit _linear request)."""
    cls = type(solver).__name__
    if cls != "NonLinearSolverNewtonRaphson":
        raise RuntimeError(f"unexpected non-linear solver class {cls}")
    return {"type": "newton_raphson",
            "type_source": "SolidMechanicsModel::getDefaultSolverOptions "
                           "(solid_mechanics_model.cc:146-160), bindings class " + cls,
            "max_iterations": max_iterations, "threshold": threshold,
            "convergence_type": convergence_type, **extra}


def run_static(w: FixtureWriter, model, mesh, dim: int):
    model.initFull(_analysis_method=aka._static)
    X = np.array(mesh.getNodes())
    on = boundary_nodes(X)
    A, b = A3[:dim, :dim], B3[:dim]
    u_exact = X @ A.T + b
    blocked = model.getBlockedDOFs()
    disp = model.getDisplacement()
    blocked[on, :] = True
    disp[on, :] = u_exact[on, :]
    solver = model.getNonLinearSolver()
    solver.set("max_iterations", 2)
    solver.set("threshold", 1e-12)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.solution)
    model.solveStep()
    w.parameters.update({"dirichlet": "affine u = A X + b on bounding-box faces",
                         "A": A.tolist(), "b": b.tolist(),
                         "solver": solver_settings(solver, 2, 1e-12, "solution"),
                         "nb_iterations": int(solver.getInt("nb_iterations")),
                         "error": float(solver.getReal("error"))})
    w.add("blocked_dofs", np.array(blocked, dtype=bool), ["node", "component"], "nodal",
          tolerance_class="exact")
    w.add("displacement", np.array(disp), ["node", "component"], "nodal",
          tolerance_class="linear_solve", description="solution (exact field: A X + b)")
    norms = record_forces(w, model, blocked)
    record_grad_u(w, model, mesh, dim)
    record_stress(w, model, mesh, dim)
    krec = record_matrix(w, "K", model.getDOFManager().getMatrix("K"),
                         "stiffness matrix entries (bit-exact AIJ storage)")
    measure_static_conditioning(w, krec, blocked, disp, model.getExternalForce(), norms)


def run_loaded(w: FixtureWriter, model, mesh, dim: int):
    """Equilibrium under a non-zero nodal load: f_ext + f_int -> 0 on free
    DOFs (with the wrong sign, f_ext - f_int would be ~2 f_ext)."""
    model.initFull(_analysis_method=aka._static)
    X = np.array(mesh.getNodes())
    tol = 1e-10 * max(1.0, float(np.abs(X).max()))
    left = np.abs(X[:, 0] - X[:, 0].min()) < tol
    right = np.abs(X[:, 0] - X[:, 0].max()) < tol
    blocked = model.getBlockedDOFs()
    blocked[left, :] = True
    fext = model.getExternalForce()
    fext[right, :] = LOAD3[:dim]
    solver = model.getNonLinearSolver()
    solver.set("max_iterations", 10)
    solver.set("threshold", 1e-12)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.residual)
    model.solveStep()
    w.parameters.update({"dirichlet": "x = xmin face blocked",
                         "neumann": "nodal force on every node of the x = xmax face",
                         "nodal_load": LOAD3[:dim].tolist(),
                         "solver": solver_settings(solver, 10, 1e-12, "residual"),
                         "nb_iterations": int(solver.getInt("nb_iterations")),
                         "error": float(solver.getReal("error"))})
    w.add("blocked_dofs", np.array(blocked, dtype=bool), ["node", "component"], "nodal",
          tolerance_class="exact")
    w.add("displacement", np.array(model.getDisplacement()), ["node", "component"], "nodal",
          tolerance_class="linear_solve")
    norms = record_forces(w, model, blocked)
    w.parameters["equilibrium"] = norms
    record_grad_u(w, model, mesh, dim)
    record_stress(w, model, mesh, dim)
    krec = record_matrix(w, "K", model.getDOFManager().getMatrix("K"),
                         "stiffness matrix entries (bit-exact AIJ storage)")
    measure_static_conditioning(w, krec, blocked, model.getDisplacement(),
                                model.getExternalForce(), norms)


def run_layout(w: FixtureWriter, model, mesh, dim: int):
    """Sentinel round trip: write distinct values into the stress buffer of
    every integration point through the bindings (writable numpy view of
    the C++ Array), read them back with record_stress. Value of M(r, c) at
    tuple k is 1000 k + 10 r + c + 1, stored at component r + c*dim."""
    model.initFull(_analysis_method=aka._static)
    mat = model.getMaterial(0)
    sentinel = {}
    for t in element_types(mesh, dim):
        buf = mat.getStress(t)
        n = buf.shape[0]
        raw = np.zeros((n, dim * dim))
        for k in range(n):
            for r in range(dim):
                for c in range(dim):
                    raw[k, r + c * dim] = 1000.0 * k + 10.0 * r + c + 1.0
        buf[:, :] = raw
        sentinel[tname(t)] = raw
    w.parameters["sentinel"] = ("M(r, c) of tuple k (= element * nb_quad + quad) is "
                                "1000 k + 10 r + c + 1, written at buffer component r + c*dim")
    for t in element_types(mesh, dim):
        w.add(f"stress_buffer/{tname(t)}", np.array(mat.getStress(t)), ["entry", "component"],
              "table", element_type=tname(t), tolerance_class="exact",
              description="raw binding buffer [tuple, component] read back after writing")
        w.parameters.setdefault("nb_quad", {})[tname(t)] = int(
            model.getFEEngine().getNbIntegrationPoints(t))
    record_stress(w, model, mesh, dim, tolerance_class="exact")


def run_explicit(w: FixtureWriter, model, mesh, dim: int, steps: int):
    model.initFull(_analysis_method=aka._explicit_lumped_mass)
    X = np.array(mesh.getNodes())
    L = X.max(axis=0) - X.min(axis=0)
    u = model.getDisplacement()
    v = model.getVelocity()
    u[:, :] = 1e-4 * np.sin(np.pi * (X - X.min(axis=0)) / L)[:, :dim]
    v[:, 0] = 1e-2 * np.cos(np.pi * (X[:, 1 % dim] - X[:, 1 % dim].min()) / L[1 % dim])
    dt = 0.5 * model.getStableTimeStep()
    model.setTimeStep(dt)
    rec = {k: [] for k in ("u", "v", "a", "fint", "ekin", "epot", "time")}

    def snap(t):
        rec["u"].append(np.array(model.getDisplacement()))
        rec["v"].append(np.array(model.getVelocity()))
        rec["a"].append(np.array(model.getAcceleration()))
        rec["fint"].append(np.array(model.getInternalForce()))
        rec["ekin"].append(model.getEnergy("kinetic"))
        rec["epot"].append(model.getEnergy("potential"))
        rec["time"].append(t)

    snap(0.0)
    for s in range(1, steps + 1):
        model.solveStep()
        snap(s * dt)
    w.set_steps(range(steps + 1), dt=dt, time_quantity="time")
    w.parameters.update({"dt": dt, "dt_rule": "0.5 * getStableTimeStep()",
                         "initial_conditions": "u = 1e-4 sin(pi (X-X0)/L), v_x = 1e-2 cos(pi (y-y0)/Ly)",
                         "step_0": "state before the first solveStep, as reported by the "
                                   "oracle: energy/potential is 0 there because stresses "
                                   "are only computed by the first solveStep"})
    w.add("time", np.array(rec["time"]), ["step"], "scalar", tolerance_class="exact")
    names = {"u": "displacement", "v": "velocity", "a": "acceleration", "fint": "internal_force"}
    for k, n in names.items():
        w.add(n, np.stack(rec[k]), ["step", "node", "component"], "nodal",
              tolerance_class="time_integration")
    w.add("energy/kinetic", np.array(rec["ekin"]), ["step"], "scalar",
          tolerance_class="time_integration")
    w.add("energy/potential", np.array(rec["epot"]), ["step"], "scalar",
          tolerance_class="time_integration")


class ResidualRecorder(aka.InterceptSolverCallback):
    """Records, for every Newton iteration, Akantu's own residual norms.

    * `static`: |f_ext + f_int| over free DOFs right after
      SolidMechanicsModel::assembleResidual (the DOF manager residual is
      exactly f_ext + f_int at that point; the integration scheme adds
      -M a after this callback, TimeStepSolverDefault::assembleResidual);
    * `full`: the residual tested for convergence, read at the corrector
      (after the sparse solve, which works on a copy of the residual) and,
      for the last iterate, after solveStep returns. Its norm is accumulated
      like testConvergence, so the last entry equals the solver `error`
      bit for bit, and convergence means full[-1] < threshold * full[0]."""

    def __init__(self, model, blocked):
        super().__init__(model)
        self.model = model
        self.dofm = model.getDOFManager()
        self.free = ~np.array(blocked, dtype=bool).ravel()
        self.static = []
        self.full = []
        self.static_last = []

    def _residual(self) -> np.ndarray:
        return np.array(self.dofm.getResidual().getValues(), dtype=np.float64).ravel()

    def begin_step(self):
        self.static.append([])
        self.full.append([])

    def assembleResidual(self):
        self.model.assembleResidual()
        r = self._residual()
        fs = (np.array(self.model.getExternalForce()) + np.array(self.model.getInternalForce())).ravel()
        if not np.array_equal(r, fs):
            raise RuntimeError("DOF manager residual differs from f_ext + f_int")
        self.static[-1].append(sequential_norm(r, self.free))
        self.static_vector = fs

    def corrector(self):
        self.full[-1].append(sequential_norm(self._residual(), self.free))
        aka.InterceptSolverCallback.corrector(self)

    def end_step(self, dim: int):
        self.full[-1].append(sequential_norm(self._residual(), self.free))
        self.static_last.append(self.static_vector.reshape(-1, dim))


def check_history(step: int, history: list, n_it: int, err: float) -> None:
    """The recorded convergence residuals belong to the solver's step:
    nb_iterations + 1 entries, the last equal to the solver `error`. In the
    strict build (no FMA contraction) sequential_norm reproduces
    testConvergence bit for bit; other configurations (strict-vs-default
    baselines only, never golden data) may contract `norm += x * x` into
    fused multiply-adds, which changes the sum of n <= 10^3 squares by at
    most ~n u relative: there the last entry must agree to 1e-12 relative."""
    if len(history) != n_it + 1:
        raise RuntimeError(f"step {step}: {len(history)} recorded residuals for "
                           f"{n_it} iterations")
    last = history[-1]
    if os.environ.get("AKANTU_PROBE_CONFIG") == "strict":
        ok = last == err
    else:
        ok = abs(last - err) <= 1e-12 * abs(err)
    if not ok:
        raise RuntimeError(f"step {step}: last recorded residual {last!r} differs from the "
                           f"solver error {err!r}")


def run_implicit_dynamic(w: FixtureWriter, model, mesh, dim: int, steps: int):
    model.initFull(_analysis_method=aka._implicit_dynamic)
    X = np.array(mesh.getNodes())
    xmin = X[:, 0].min()
    left = np.abs(X[:, 0] - xmin) < 1e-10
    blocked = model.getBlockedDOFs()
    blocked[left, :] = True
    v = model.getVelocity()
    v[:, 1] = 0.05 * (X[:, 0] - xmin)  # shear rate 0.05/s: yields after ~2 steps
    dt = 1e-4
    model.setTimeStep(dt)
    solver = model.getNonLinearSolver()
    solver.set("max_iterations", 50)
    solver.set("threshold", 1e-10)
    solver.set("convergence_type", aka.SolveConvergenceCriteria.residual)
    cb = ResidualRecorder(model, blocked)
    dofm = model.getDOFManager()
    rec = {k: [] for k in ("u", "v", "a", "ekin", "epot")}
    iters, errors, cond_j, norm_j, rates = [], [], [], [], []
    for _ in range(steps):
        cb.begin_step()
        model.solveStep(cb)
        cb.end_step(dim)
        n_it = int(solver.getInt("nb_iterations"))
        err = float(solver.getReal("error"))
        check_history(len(rec["u"]) + 1, cb.full[-1], n_it, err)
        iters.append(n_it)
        errors.append(err)
        jr, jc, jv, jn, jsym = aij(dofm.getMatrix("J"))
        jd = dense(jr, jc, jv, jn, jsym)
        free = cb.free
        jff = jd[np.ix_(free, free)]
        cond_j.append(cond2(jff))
        norm_j.append(float(np.linalg.norm(jff, 2)))
        h = cb.full[-1]
        tail = [h[k + 1] / h[k] for k in range(len(h) - 1) if h[k] > 0]
        rates.append(max(tail) if tail else 0.0)
        rec["u"].append(np.array(model.getDisplacement()))
        rec["v"].append(np.array(model.getVelocity()))
        rec["a"].append(np.array(model.getAcceleration()))
        rec["ekin"].append(model.getEnergy("kinetic"))
        rec["epot"].append(model.getEnergy("potential"))
    w.set_steps(range(1, steps + 1), dt=dt)
    w.parameters.update({"dt": dt, "integration_scheme": "default of _implicit_dynamic (trapezoidal rule)",
                         "solver": solver_settings(
                             solver, 50, 1e-10, "residual",
                             normalisation="converged when |r_k| < threshold * |r_0| "
                                           "(non_linear_solver_newton_raphson.cc:128-138), "
                                           "|.| = 2-norm over free DOFs"),
                         "bc": "x = xmin blocked; v_y = 0.05 (x - xmin) initially",
                         "conditioning": {
                             "cond2_J_free_per_step": cond_j,
                             "norm2_J_free_per_step": norm_j,
                             "method": "numpy SVD of the Newton matrix J[free, free] (bit-exact AIJ) "
                                       "as left by the last Jacobian assembly of each step"},
                         "newton_contraction": {
                             "max_ratio_per_step": rates,
                             "definition": "max_k |r_{k+1}| / |r_k| over the full residual history "
                                           "of the step (linear convergence: the plastic tangent "
                                           "is not the consistent one)"}})
    w.add("blocked_dofs", np.array(blocked, dtype=bool), ["node", "component"], "nodal",
          tolerance_class="exact")
    for k, n in {"u": "displacement", "v": "velocity", "a": "acceleration"}.items():
        w.add(n, np.stack(rec[k]), ["step", "node", "component"], "nodal",
              tolerance_class="nonlinear_solve")
    w.add("energy/kinetic", np.array(rec["ekin"]), ["step"], "scalar", tolerance_class="nonlinear_solve")
    w.add("energy/potential", np.array(rec["epot"]), ["step"], "scalar", tolerance_class="nonlinear_solve")
    w.add("newton/nb_iterations", np.array(iters, dtype=np.int64), ["step"], "scalar",
          tolerance_class="exact")
    w.add("newton/error", np.array(errors), ["step"], "scalar", tolerance_class="nonlinear_solve",
          description="solver `error` after convergence: |r_final| (2-norm over free DOFs, "
                      "absolute, N)")
    for name, hist, desc in (
            ("newton/residual_norm", cb.full,
             "Akantu's convergence residual |f_ext + f_int - M a| over free DOFs at r_0 and after "
             "every Newton iteration (nb_iterations + 1 entries per step; the last equals "
             "newton/error bit for bit)"),
            ("newton/static_residual_norm", cb.static,
             "pre-inertia part |f_ext + f_int| over free DOFs at every residual assembly "
             "(the inertial term -M a is added after the solver callback)")):
        offsets = np.cumsum([0] + [len(h) for h in hist]).astype(np.int64)
        w.add(f"{name}/offsets", offsets, ["entry"], "ragged_offsets", tolerance_class="exact")
        w.add(name, np.array([x for h in hist for x in h]), ["iteration"], "ragged_values",
              ragged={"offsets": f"{name}/offsets", "row_axis": "step"},
              tolerance_class="nonlinear_solve", description=desc)
    w.add("newton/static_residual_last", np.stack(cb.static_last), ["step", "node", "component"],
          "nodal", tolerance_class="nonlinear_solve",
          description="f_ext + f_int at the last residual assembly of each step; at convergence "
                      "it equals M a on free DOFs up to newton/error")
    record_matrix(w, "M", dofm.getMatrix("M"), "consistent mass matrix (bit-exact AIJ storage)")


def run_case(root: Path, src: Path, name: str, spec: dict) -> None:
    dim = spec["dim"]
    with tempfile.TemporaryDirectory() as tmpd:
        tmp = Path(tmpd)
        mat = tmp / "material.dat"
        mat.write_text(spec.get("material", ELASTIC), encoding="utf-8")
        aka.parseInput(str(mat))
        w = FixtureWriter(root, f"model/{name}", f"SolidMechanicsModel {spec['kind']} run on "
                          f"{spec['mesh']} ({dim}D)", PROBE, VERSION)
        w.tags += ["model", spec["kind"]]
        w.parameters["probe_semantics"] = (
            "tensors [..., row, col] = M(row, col) (per-tuple transpose of the column-major "
            "binding buffer); static residual = f_ext + f_int")
        w.add_input_file("mesh", src / spec["mesh"], source="upstream:" + spec["mesh"])
        w.add_input_file("material", mat, source="generated:model_probe.py")
        mesh = aka.Mesh(dim)
        mesh.read(str(src / spec["mesh"]))
        model = aka.SolidMechanicsModel(mesh)
        w.parameters["spatial_dimension"] = dim
        kind = spec["kind"]
        if kind == "static":
            run_static(w, model, mesh, dim)
        elif kind == "loaded":
            run_loaded(w, model, mesh, dim)
        elif kind == "layout":
            run_layout(w, model, mesh, dim)
        elif kind == "explicit":
            run_explicit(w, model, mesh, dim, spec["steps"])
        else:
            run_implicit_dynamic(w, model, mesh, dim, spec["steps"])
        record_mesh(w, mesh, dim)
        print("  model/" + name, flush=True)
        w.finish()


def main() -> int:
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} <fixtures_root> <akantu_src> [case ...]", file=sys.stderr)
        return 2
    root, src = Path(sys.argv[1]), Path(sys.argv[2])
    wanted = sys.argv[3:] or list(CASES)
    if len(wanted) == 1:
        try:
            run_case(root, src, wanted[0], CASES[wanted[0]])
        except Exception as e:  # noqa: BLE001 - report the failing case
            print(f"model_probe: {wanted[0]} failed: {e!r}", file=sys.stderr)
            return 1
        return 0
    # One process per case: Akantu's static parser accumulates the sections
    # of every parseInput() call, so a second material file in the same
    # process would not replace the first.
    failures = 0
    for name in wanted:
        r = subprocess.run([sys.executable, __file__, str(root), str(src), name], check=False)
        failures += int(r.returncode != 0)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
