#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The Python part of
# the set-up benchmark suite (no upstream counterpart; the loops follow
# examples/python/solid_mechanics_model/dynamics and custom-material).
"""Cost of driving akantu from Python.

Same command line and JSON lines as the Rust suites
(``docs/briefs/perf/SUITE.md``)::

    suite_python.py --list
    suite_python.py --case <name>[,<name>...]|all --size s|m|l [--reps N]
                    [--out FILE] [--loop-seconds S] [--threads N]

``explicit_tri3`` runs exactly the loop of the Rust case
``suite_setup py_twin_explicit_tri3`` (same mesh, material, initial
displacement, three warm-up steps, same step count) and prints the same
FNV-1a digest of the displacement and velocity (of the main model, then of
the tiny model of ``solve_step_tiny``): equal digests show that the two
drivers computed the same bits.
"""

import argparse
import json
import math
import os
import subprocess
import sys
import tempfile
import time

import numpy as np

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
MASK = (1 << 64) - 1


def fnv1a(chunks):
    """FNV-1a 64 over the bytes of each chunk (``bytes`` or arrays)."""
    h = FNV_OFFSET
    for c in chunks:
        if isinstance(c, np.ndarray):
            c = np.ascontiguousarray(c, dtype="<f8").tobytes()
        for b in c:
            h = ((h ^ b) * FNV_PRIME) & MASK
    return h


def pick(size, s, m, l):
    return {"s": s, "m": m, "l": l}[size]


# ---------------------------------------------------------------- meshes
def tri3_msh22(n):
    """MSH 2.2 text of the Tri3 unit square, identical (node values,
    element order, physical groups) to crates/akantu-bench
    suite_setup/meshgen.rs ``msh22(Grid::Tri3, n)``."""
    h = 1.0 / float(n)

    def nid(i, j):
        return j * (n + 1) + i + 1

    lines = ["$MeshFormat", "2.2 0 8", "$EndMeshFormat", "$PhysicalNames", "5"]
    for k, name in enumerate(["xmin", "xmax", "ymin", "ymax"]):
        lines.append('1 %d "%s"' % (k + 1, name))
    lines += ['2 5 "body"', "$EndPhysicalNames", "$Nodes", str((n + 1) ** 2)]
    for j in range(n + 1):
        for i in range(n + 1):
            lines.append(
                "%d %r %r %r" % (nid(i, j), float(i) * h, float(j) * h, 0.0)
            )
    lines.append("$EndNodes")
    sides = [
        [(nid(0, k), nid(0, k + 1)) for k in range(n)],
        [(nid(n, k), nid(n, k + 1)) for k in range(n)],
        [(nid(k, 0), nid(k + 1, 0)) for k in range(n)],
        [(nid(k, n), nid(k + 1, n)) for k in range(n)],
    ]
    elems = []
    for tag, side in enumerate(sides):
        for a, b in side:
            elems.append("1 2 %d %d %d %d" % (tag + 1, tag + 1, a, b))
    for j in range(n):
        for i in range(n):
            elems.append(
                "2 2 5 5 %d %d %d" % (nid(i, j), nid(i + 1, j), nid(i + 1, j + 1))
            )
            elems.append(
                "2 2 5 5 %d %d %d" % (nid(i, j), nid(i + 1, j + 1), nid(i, j + 1))
            )
    lines += ["$Elements", str(len(elems))]
    lines += ["%d %s" % (k + 1, e) for k, e in enumerate(elems)]
    lines += ["$EndElements", ""]
    return "\n".join(lines)


STEEL = """material elastic [
    name = steel
    rho = 7800
    E   = 2.1e11
    nu  = 0.3
]
"""

LOCAL = """material local_elastic [
    name = steel
    rho = 7800
    E   = 2.1e11
    nu  = 0.3
]
"""


def pulse(x):
    """The Rust suite's ``cases_dist::pulse`` (same operation order)."""
    l = 0.25
    a = 1e-4
    x = x - 0.5
    k = 0.1 * 2.0 * math.pi * 3.0 / l
    return a * math.sin(k * x) * math.exp(-(k * x) * (k * x) / (l * l))


class Ctx:
    def __init__(self, args, aka, tmp):
        self.size = args.size
        self.loop_seconds = args.loop_seconds
        self.aka = aka
        self.tmp = tmp


class Out:
    def __init__(self):
        self.phases = []
        self.n_elements = 0
        self.n_nodes = 0
        self.n_dofs = 0
        self.digest = 0
        self.extra = {}

    def phase(self, name, f):
        t = time.perf_counter()
        r = f()
        self.phases.append((name, 1, time.perf_counter() - t))
        return r

    def loop_phase(self, ctx, name, calls, f):
        for _ in range(3):
            f()
        t = time.perf_counter()
        n = 0
        while True:
            for _ in range(max(calls, 1)):
                f()
            n += max(calls, 1)
            if ctx.loop_seconds is None or time.perf_counter() - t >= ctx.loop_seconds:
                break
        self.phases.append((name, n, time.perf_counter() - t))


def tri3_n(size):
    """crates/akantu-bench cases_io::tri3_n."""
    return pick(size, 8, 200, 566)


def tri3_steps(size):
    """crates/akantu-bench cases_io::tri3_steps."""
    return pick(size, 5, 400, 100)


def write(ctx, name, text):
    p = os.path.join(ctx.tmp, name)
    with open(p, "w") as f:
        f.write(text)
    return p


def explicit_model(ctx, out, n, material=STEEL, pulse_loop=True):
    aka = ctx.aka
    mesh_file = write(ctx, "square_%d.msh" % n, tri3_msh22(n))
    dat = write(ctx, "material.dat", material)

    def build():
        aka.parseInput(dat)
        mesh = aka.Mesh(2)
        mesh.read(mesh_file)
        return mesh

    mesh = out.phase("mesh", build)
    out.n_nodes = mesh.getNbNodes()
    out.n_elements = mesh.getNbElement(element_type=aka._triangle_3)
    out.n_dofs = 2 * out.n_nodes

    def init():
        model = aka.SolidMechanicsModel(mesh)
        model.initFull(_analysis_method=aka._explicit_lumped_mass)
        model.setTimeStep(model.getStableTimeStep() * 0.8)
        return model

    model = out.phase("init", init)
    u = model.getDisplacement()
    x = mesh.getNodes()
    if pulse_loop:
        for i in range(mesh.getNbNodes()):
            u[i, 0] = pulse(x[i, 0])
    return mesh, model


# ----------------------------------------------------------------- cases
def case_import(ctx):
    """`import akantu` in a fresh interpreter, against an empty one."""
    out = Out()
    calls = pick(ctx.size, 2, 10, 10)
    env = dict(os.environ, MPLBACKEND="Agg")

    def run(code):
        subprocess.run([sys.executable, "-c", code], check=True, env=env)

    out.loop_phase(ctx, "python_startup", calls, lambda: run("pass"))
    out.loop_phase(ctx, "import_akantu", calls, lambda: run("import akantu"))
    out.loop_phase(
        ctx, "import_numpy", calls, lambda: run("import numpy")
    )
    return out


def case_explicit(ctx):
    """The Rust twin's loop driven from Python."""
    out = Out()
    mesh, model = explicit_model(ctx, out, tri3_n(ctx.size))
    out.loop_phase(ctx, "solve_step", tri3_steps(ctx.size), model.solveStep)
    keep = (out.n_elements, out.n_nodes, out.n_dofs)
    tiny_out = Out()
    _, tiny = explicit_model(ctx, tiny_out, 4)
    out.loop_phase(
        ctx, "solve_step_tiny", pick(ctx.size, 20, 300000, 300000), tiny.solveStep
    )
    # the tiny model's state too (the Rust twin hashes the same arrays)
    out.digest = fnv1a(
        [
            model.getDisplacement(),
            model.getVelocity(),
            tiny.getDisplacement(),
            tiny.getVelocity(),
        ]
    )
    out.n_elements, out.n_nodes, out.n_dofs = keep
    return out


def case_fields(ctx):
    """NumPy access to the model's fields: views, copies, Python and
    vectorised element-wise writes, a material's stress."""
    aka = ctx.aka
    out = Out()
    mesh, model = explicit_model(ctx, out, tri3_n(ctx.size), pulse_loop=False)
    calls = pick(ctx.size, 20, 20000, 20000)
    out.loop_phase(ctx, "get_displacement", calls, model.getDisplacement)
    out.loop_phase(
        ctx, "copy_displacement", pick(ctx.size, 20, 2000, 500),
        lambda: np.array(model.getDisplacement()),
    )
    u = model.getDisplacement()
    x = mesh.getNodes()
    nb = mesh.getNbNodes()

    def python_loop():
        for i in range(nb):
            u[i, 0] = pulse(x[i, 0])

    out.loop_phase(ctx, "python_loop_set", pick(ctx.size, 2, 3, 1), python_loop)
    xs = x[:, 0]

    def numpy_set():
        k = 0.1 * 2.0 * math.pi * 3.0 / 0.25
        z = k * (xs - 0.5)
        u[:, 1] = 1e-4 * np.sin(z) * np.exp(-z * z / (0.25 * 0.25))

    out.loop_phase(ctx, "numpy_set", pick(ctx.size, 2, 200, 50), numpy_set)
    mat = model.getMaterial(0)
    out.loop_phase(
        ctx, "get_stress", calls, lambda: mat.getStress(aka._triangle_3)
    )
    out.digest = fnv1a([u[:, 0].copy()])
    return out


def case_dirichlet(ctx):
    """applyBC with a Python DirichletFunctor (one call per node) against
    the built-in FixedValue, on every node of the square."""
    aka = ctx.aka
    out = Out()
    _, model = explicit_model(ctx, out, tri3_n(ctx.size), pulse_loop=False)

    class PyFixedValue(aka.DirichletFunctor):
        def __init__(self, value, axis):
            super().__init__(axis)
            self.value = value
            self.axis = int(axis)

        def __call__(self, node, flags, primal, coord):
            primal[self.axis] = self.value
            flags[self.axis] = True

    calls = pick(ctx.size, 2, 20, 5)
    py = PyFixedValue(1e-3, aka._x)
    out.loop_phase(ctx, "python_functor", calls, lambda: model.applyBC(py, "body"))
    builtin = aka.FixedValue(2e-3, aka._y)
    out.loop_phase(
        ctx, "builtin_functor", calls * 10, lambda: model.applyBC(builtin, "body")
    )
    out.digest = fnv1a(
        [model.getDisplacement(), model.getBlockedDOFs().astype("<f8")]
    )
    return out


def case_neumann(ctx):
    """applyBC with a Python NeumannFunctor (one call per integration
    point) against the built-in FromTraction, on the four sides."""
    aka = ctx.aka
    out = Out()
    _, model = explicit_model(ctx, out, tri3_n(ctx.size), pulse_loop=False)
    trac = np.array([1.0e6, 0.5e6])

    class PyTraction(aka.NeumannFunctor):
        def __call__(self, quad_point, dual, coord, normals):
            dual[:] = trac

    calls = pick(ctx.size, 2, 200, 50)
    py = PyTraction()
    sides = ["xmin", "xmax", "ymin", "ymax"]

    def apply_py():
        for s in sides:
            model.applyBC(py, s)

    builtin = aka.FromTraction(trac)

    def apply_builtin():
        for s in sides:
            model.applyBC(builtin, s)

    out.loop_phase(ctx, "python_functor", calls, apply_py)
    out.loop_phase(ctx, "builtin_functor", calls * 10, apply_builtin)
    out.digest = fnv1a([model.getExternalForce()])
    return out


def register_local_elastic(aka):
    class LocalElastic(aka.Material):
        """examples/python/solid_mechanics_model/custom-material."""

        def __init__(self, model, _id):
            super().__init__(model, _id)
            super().registerParamReal(
                "E", aka._pat_readable | aka._pat_parsable, "Youngs modulus"
            )
            super().registerParamReal(
                "nu", aka._pat_readable | aka._pat_parsable, "Poisson ratio"
            )

        def initMaterial(self):
            nu = self.getReal("nu")
            E = self.getReal("E")
            self.lame_lambda = nu * E / ((1.0 + nu) * (1.0 - 2.0 * nu))
            self.lame_mu = E / (2.0 * (1.0 + nu))
            super().initMaterial()

        def getPushWaveSpeed(self, element):
            rho = self.getReal("rho")
            return np.sqrt((self.lame_lambda + 2 * self.lame_mu) / rho)

        @staticmethod
        def computeEpsilon(grad_u):
            return 0.5 * (grad_u + np.einsum("aij->aji", grad_u))

        def computeStress(self, el_type, ghost_type):
            grad_u = self.getGradU(el_type, ghost_type)
            sigma = self.getStress(el_type, ghost_type)
            n_quads = grad_u.shape[0]
            grad_u = grad_u.reshape((n_quads, 2, 2))
            epsilon = self.computeEpsilon(grad_u)
            sigma = sigma.reshape((n_quads, 2, 2))
            trace = np.einsum("aii->a", grad_u)
            sigma[:, :, :] = (
                np.einsum("a,ij->aij", trace, self.lame_lambda * np.eye(2))
                + 2.0 * self.lame_mu * epsilon
            )

        def computeTangentModuli(self, el_type, tangent_matrix, ghost_type):
            n_quads = tangent_matrix.shape[0]
            tangent = tangent_matrix.reshape(n_quads, 3, 3)
            tangent[:, 0, 0] = self.lame_lambda + 2 * self.lame_mu
            tangent[:, 1, 1] = self.lame_lambda + 2 * self.lame_mu
            tangent[:, 0, 1] = self.lame_lambda
            tangent[:, 1, 0] = self.lame_lambda
            tangent[:, 2, 2] = self.lame_mu

        def computePotentialEnergy(self, el_type):
            sigma = self.getStress(el_type)
            grad_u = self.getGradU(el_type)
            nquads = sigma.shape[0]
            stress = sigma.reshape(nquads, 2, 2)
            grad_u = grad_u.reshape((nquads, 2, 2))
            epsilon = self.computeEpsilon(grad_u)
            energy_density = self.getPotentialEnergy(el_type)
            energy_density[:, 0] = 0.5 * np.einsum("aij,aij->a", stress, epsilon)

    def allocator(_dim, unused, model, _id):
        return LocalElastic(model, _id)

    aka.MaterialFactory.getInstance().registerAllocator("local_elastic", allocator)


def case_material(ctx):
    """The explicit loop with the elastic law written in Python (NumPy per
    element type and step) against the built-in elastic law."""
    out = Out()
    register_local_elastic(ctx.aka)
    n = tri3_n(ctx.size)
    steps = tri3_steps(ctx.size)
    _, model = explicit_model(ctx, out, n, material=LOCAL)
    out.loop_phase(ctx, "solve_step_python_law", steps, model.solveStep)
    py_digest = fnv1a([model.getDisplacement(), model.getVelocity()])
    other = Out()
    _, ref = explicit_model(ctx, other, n)
    out.loop_phase(ctx, "solve_step_builtin_law", steps, ref.solveStep)
    out.digest = py_digest
    out.extra["builtin_digest_equal"] = float(
        py_digest == fnv1a([ref.getDisplacement(), ref.getVelocity()])
    )
    return out


CASES = [
    ("import", "interpreter start-up, `import akantu` and `import numpy` in fresh processes", case_import),
    ("explicit_tri3", "Python loop of solveStep, the twin of suite_setup py_twin_explicit_tri3 (same digest)", case_explicit),
    ("fields_tri3", "NumPy views and copies of fields, Python and vectorised writes, a law's stress", case_fields),
    ("dirichlet_tri3", "applyBC with a Python DirichletFunctor (a call per node) against FixedValue", case_dirichlet),
    ("neumann_tri3", "applyBC with a Python NeumannFunctor (a call per integration point) against FromTraction", case_neumann),
    ("material_tri3", "explicit loop with a Python elastic law (NumPy per type and step) against the built-in law", case_material),
]


def commit():
    c = os.environ.get("AKANTU_SUITE_COMMIT")
    if c:
        return c
    try:
        return subprocess.run(
            ["git", "rev-parse", "--short=9", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--list", action="store_true")
    p.add_argument("--case")
    p.add_argument("--size", choices=["s", "m", "l"], default="m")
    p.add_argument("--reps", type=int, default=1)
    p.add_argument("--out")
    p.add_argument("--loop-seconds", type=float)
    p.add_argument("--threads", type=int, default=1)
    args = p.parse_args(argv)
    if args.list:
        for name, desc, _ in CASES:
            print("%-26s %s" % (name, desc))
        return 0
    if not args.case:
        p.error("--case or --list is required")
    os.environ.setdefault("MPLBACKEND", "Agg")
    import akantu as aka

    names = [c[0] for c in CASES] if args.case == "all" else args.case.split(",")
    by_name = {c[0]: c for c in CASES}
    sink = open(args.out, "a") if args.out else sys.stdout
    rev = commit()
    with tempfile.TemporaryDirectory(prefix="akantu-suite-python-") as tmp:
        ctx = Ctx(args, aka, tmp)
        cwd = os.getcwd()
        os.chdir(tmp)
        try:
            for name in names:
                if name not in by_name:
                    p.error("unknown case %s (see --list)" % name)
                for rep in range(args.reps):
                    out = by_name[name][2](ctx)
                    for phase, calls, seconds in out.phases:
                        line = {
                            "suite": "python", "case": name, "size": args.size,
                            "rep": rep, "phase": phase, "calls": calls,
                            "seconds": seconds, "n_elements": out.n_elements,
                            "n_nodes": out.n_nodes, "n_dofs": out.n_dofs,
                            "ranks": 1, "threads": args.threads,
                            "digest": "%016x" % out.digest, "commit": rev,
                            "features": "python",
                        }
                        line.update(out.extra)
                        sink.write(json.dumps(line) + "\n")
                    sink.flush()
        finally:
            os.chdir(cwd)
    if args.out:
        sink.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
