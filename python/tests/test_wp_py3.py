# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests: behaviours of upstream's module confirmed with the
# oracle (probes M11-04 and following):
# python/py_parser.cc
# (`ParameterRegistry.get`), python/py_mesh.cc (the three
# `ElementTypeMapArray.initialize` overloads), python/py_aka_common.cc
# (`__initialize`: `initialize(argc, argv)` on `sys.argv` at import).
"""Small Python rows: PY-040, PY-055, DOC-004 / PY-004."""

import os
import subprocess
import sys

import numpy as np
import pytest

import akantu as aka
from _paths import MESHES, PARSER_FIXTURES


@pytest.fixture(autouse=True)
def _dcb(upstream_dir):
    upstream_dir(
        **{
            "mesh_dcb_2d.msh": MESHES
            / "test/test_python_interface/mesh_dcb_2d.msh",
            "elastic.dat": PARSER_FIXTURES
            / "test/test_python_interface/elastic.dat",
        }
    )


def static_model():
    aka.parseInput("elastic.dat")
    mesh = aka.Mesh(2)
    mesh.read("mesh_dcb_2d.msh")
    model = aka.SolidMechanicsModel(mesh)
    model.initFull(_analysis_method=aka._static)
    return mesh, model


# --- PY-040: get() needs a readable *and* writable parameter ---------


def test_py040_get_of_a_readable_only_parameter_raises_like_upstream():
    """Oracle probe M11-04: upstream's generic ``get`` reads through the
    mutable ``Parameter::get<T>()``, which checks readable and writable;
    ``lambda`` of an elastic law is readable only, so ``get("lambda")``
    raises ``akantu.Exception`` ("Parameter lambda is not accessible") while
    ``getReal("lambda")`` returns it; ``E`` (modifiable) works both ways."""
    _, model = static_model()
    mat = model.getMaterial(0)
    assert mat.get("E") == 71e9
    assert mat.getReal("E") == 71e9
    with pytest.raises(aka.Exception, match="Parameter lambda is not accessible"):
        mat.get("lambda")
    lam = mat.getReal("lambda")
    assert isinstance(lam, float) and lam > 0.0


def test_py040_python_registered_parameters_follow_the_same_rule():
    p = aka.Parsable(aka.ParserType.material, "py040")
    p.registerParamReal(1.5, "ro", aka._pat_readable, "readable only")
    p.registerParamReal(2.5, "rw", aka._pat_modifiable, "modifiable")
    assert p.get("rw") == 2.5
    assert p.getReal("ro") == 1.5
    with pytest.raises(aka.Exception, match="Parameter ro is not accessible"):
        p.get("ro")


# --- PY-055: the FEEngine overloads of initialize ---------------------


def test_py055_initialize_with_an_fe_engine_in_every_upstream_form():
    """Oracle probe M11-05: ``initialize(fe, nb_component=2)``,
    ``initialize(fe_engine=fe, nb_component=2)`` and the positional
    ``initialize(fe, _casper, 2)`` all give one (1600, 2) block upstream
    (400 quadrangle_8 elements, 4 integration points each)."""
    mesh, model = static_model()
    fe = model.getFEEngine()
    forms = [
        lambda e: e.initialize(fe, nb_component=2),
        lambda e: e.initialize(fe_engine=fe, nb_component=2),
        lambda e: e.initialize(fe, aka._casper, 2),
        lambda e: e.initialize(mesh=fe, nb_component=2),
        lambda e: e.initialize(fe, aka._casper, 2, -2, aka._ek_not_defined,
                               False, False, 0.5, False),
    ]
    for i, form in enumerate(forms):
        e = aka.ElementTypeMapArrayReal()
        form(e)
        a = e(aka._quadrangle_8)
        assert a.shape == (1600, 2), i
        assert np.all(a == (0.5 if i == 4 else 0.0)), i


def test_py055_a_mesh_takes_keyword_arguments_only():
    mesh, model = static_model()
    fe = model.getFEEngine()
    e = aka.ElementTypeMapArrayReal()
    e.initialize(mesh, nb_component=2, with_nb_element=True)
    assert e(aka._quadrangle_8).shape == (400, 2)
    # upstream: no overload takes a Mesh with positional arguments after it,
    # nor a Mesh as `fe_engine`
    with pytest.raises(TypeError):
        aka.ElementTypeMapArrayReal().initialize(mesh, aka._casper, 2)
    with pytest.raises(TypeError):
        aka.ElementTypeMapArrayReal().initialize(fe_engine=mesh)
    with pytest.raises(TypeError):
        aka.ElementTypeMapArrayReal().initialize(fe, aka._casper,
                                                 nb_component=2,
                                                 ghost_type=aka._casper)
    with pytest.raises(TypeError):
        aka.ElementTypeMapArrayReal().initialize(fe, mesh=fe)


# --- DOC-004 / PY-004: sys.argv is parsed at import ----------

ARGV_PROBE = """
import sys
argv_before = list(sys.argv)
import akantu as aka
print("argv", sys.argv == argv_before)
print("level", int(aka.debug.getDebugLevel()))
try:
    sec = aka.getStaticParser().getSubSections(aka.ParserType.material)
    name = sec.getParameter("name").getValue()
except Exception:
    name = "-"
print("material", name)
"""


def run_probe(tmp_path, args, env_extra=None):
    """Run ``ARGV_PROBE`` in a fresh interpreter with ``args`` appended to
    its command line (``sys.argv[1:]``), the akantu package this test
    imported first on the path."""
    script = tmp_path / "argv_probe.py"
    script.write_text(ARGV_PROBE)
    env = dict(os.environ)
    env.pop("AKA_DEBUG_LEVEL", None)
    env.pop("AKA_SEED", None)
    env.update(env_extra or {})
    env["PYTHONPATH"] = os.pathsep.join(
        [os.path.dirname(os.path.dirname(aka.__file__)),
         env.get("PYTHONPATH", "")])
    r = subprocess.run([sys.executable, str(script), *args],
                       capture_output=True, text=True, cwd=str(tmp_path),
                       timeout=120, env=env)
    assert r.returncode == 0, r.stderr
    return dict(line.split(" ", 1) for line in r.stdout.splitlines())


def test_doc004_import_parses_the_akantu_options_of_sys_argv(tmp_path):
    """``import akantu`` runs ``__initialize()``: upstream
    ``initialize(argc, argv)`` on a copy of ``sys.argv``
    (``py_aka_common.cc:59-72``): ``--aka_debug_level`` sets the debug
    level, ``--aka_input_file`` reads the file into the static parser,
    unknown arguments are ignored and ``sys.argv`` itself is unchanged."""
    (tmp_path / "in.dat").write_text(
        "material elastic [\n  name = from_argv\n  E = 1\n  nu = 0.3\n  rho = 1\n]\n")
    out = run_probe(tmp_path, [])
    assert out == {"argv": "True", "level": "4", "material": "-"}
    out = run_probe(tmp_path, ["--my_option", "x", "--aka_debug_level", "5",
                               "--aka_input_file", "in.dat"])
    assert out == {"argv": "True", "level": "5", "material": "from_argv"}


def test_doc004_aka_debug_level_wins_over_the_option(tmp_path):
    out = run_probe(tmp_path, ["--aka_debug_level", "5"],
                    {"AKA_DEBUG_LEVEL": "8"})
    assert out["level"] == "8"


def test_doc004_a_malformed_akantu_option_fails_the_import(tmp_path):
    script = tmp_path / "bad.py"
    script.write_text("import akantu\n")
    env = dict(os.environ)
    env["PYTHONPATH"] = os.pathsep.join(
        [os.path.dirname(os.path.dirname(aka.__file__)),
         env.get("PYTHONPATH", "")])
    r = subprocess.run([sys.executable, str(script), "--aka_debug_level",
                        "not-a-number"], capture_output=True, text=True,
                       cwd=str(tmp_path), timeout=120, env=env)
    assert r.returncode != 0


# --- PY-045: SolverVector.getValues() is a live view ---------


def test_py045_residual_view_is_writable_and_follows_the_residual():
    """Oracle probe M11-07: upstream's ``getValues()`` is a writable view of
    the residual (``reference_internal``, "Is not copied"): an array kept
    after ``model.assembleResidual()`` equals a fresh call."""
    mesh, model = static_model()
    dof = model.getDOFManager()
    res = dof.getResidual()
    v = res.getValues()
    assert v.shape == (2 * mesh.getNbNodes(), 1) and v.dtype == np.float64
    assert v.flags.writeable
    model.applyBC(aka.FixedValue(0.0, aka._x), "edge")
    model.applyBC(aka.FixedValue(0.0, aka._y), "edge")
    model.getDisplacement()[5, 0] = 1e-3
    model.assembleResidual()
    fresh = dof.getResidual().getValues()
    assert np.abs(v).max() > 0.0
    assert np.array_equal(v, fresh)  # upstream: True (the port: was False)
    assert np.shares_memory(v, fresh)
    out = np.zeros((mesh.getNbNodes(), 2))
    dof.getArrayPerDOFs("displacement", res, out)
    assert np.array_equal(np.sort(out.ravel()), np.sort(v.ravel()))
    dof.zeroResidual()
    assert not v.any()
    f = np.zeros((mesh.getNbNodes(), 2))
    f[3, 1] = 2.0
    dof.assembleToResidual("displacement", f)
    assert np.array_equal(np.sort(v.ravel()), np.sort(f.ravel()))
    # writes land in the residual
    v[:] = 0.5
    dof.getArrayPerDOFs("displacement", res, out)
    assert np.all(out == 0.5)


def test_py045_residual_view_inside_a_solver_callback():
    """A view fetched before a solve follows the residual inside the
    solve's callbacks; a view fetched inside a callback (through the solve
    access) is the same storage, and a write through it is what the solve
    then uses (equal, bit for bit, to assembling the same force with
    ``assembleToResidual``)."""
    def solve(through_view):
        mesh, model = static_model()
        model.applyBC(aka.FixedValue(0.0, aka._x), "edge")
        model.applyBC(aka.FixedValue(0.0, aka._y), "edge")
        edge = set(mesh.getElementGroup("edge").getNodeGroup().getNodes().ravel())
        node = next(n for n in range(mesh.getNbNodes()) if n not in edge)
        dof = model.getDOFManager()
        retained = dof.getResidual().getValues()
        f = np.zeros((mesh.getNbNodes(), 2))
        f[node, 1] = 1.0
        # the equation of (node, y), read through the view
        dof.zeroResidual()
        dof.assembleToResidual("displacement", f)
        eq = int(np.flatnonzero(retained[:, 0])[0])
        dof.zeroResidual()
        seen = []

        class Load(aka.InterceptSolverCallback):
            def assembleResidual(self):
                model.assembleResidual()
                inside = dof.getResidual().getValues()
                seen.append(np.array_equal(inside, retained)
                            and np.shares_memory(inside, retained))
                if through_view:
                    inside[eq, 0] += 1.0
                else:
                    dof.assembleToResidual("displacement", f)

        model.solveStep(Load(model))
        assert seen and all(seen)
        return model.getDisplacement().copy()

    a, b = solve(True), solve(False)
    assert np.abs(a).max() > 0.0
    assert np.array_equal(a, b)


# --- PY-086: the stand-alone ContactDetector -----------------

DETECTION = "examples/python/contact_mechanics_model/compression"


def detection_inputs(tmp_path):
    from _paths import REPO

    for name, src in {
        "detection-explicit.msh": REPO / "fixtures/mesh_io" / DETECTION
        / "detection-explicit/inputs/detection-explicit.msh",
        "detection-explicit.dat": PARSER_FIXTURES / DETECTION
        / "detection-explicit.dat",
    }.items():
        (tmp_path / name).write_bytes(src.read_bytes())


def test_py086_standalone_detector_from_the_contact_detector_section(tmp_path):
    """Oracle probe M11-08: ``ContactDetector(mesh)`` builds a detector from
    the first ``contact_detector`` section of the static parser
    (``contact_detector.cc:28-50``) on the mesh's node coordinates."""
    import py11_akantu_test_common as tc

    detection_inputs(tmp_path)
    aka.parseInput(str(tmp_path / "detection-explicit.dat"))
    mesh = aka.Mesh(2)
    mesh.read(str(tmp_path / "detection-explicit.msh"))
    for d in (aka.ContactDetector(mesh), aka.ContactDetector(mesh, "other"),
              aka.ContactDetector(mesh, id="other")):
        kind, ptol, iters, etol, n, has_sel = tc.contactDetectorSummary(d)
        assert (kind, ptol, iters, etol) == ("Explicit", 1e-2, 1000, 1e-1)
        assert n == mesh.getNbNodes() and not has_sel
    d = aka.ContactDetector(mesh)
    d.setSurfaceSelector(aka.PhysicalSurfaceSelector(mesh))
    assert tc.contactDetectorSummary(d)[5]
    # upstream's PhysicalSurfaceSelector creates the group contact_surface
    # (master and slave surfaces)
    g = mesh.getElementGroup("contact_surface")
    top = mesh.getElementGroup("contact_top")
    bottom = mesh.getElementGroup("contact_bottom")
    n = sum(len(g.getElements()(t)) for t in g.getElements().elementTypes(dim=1))
    nt = sum(len(top.getElements()(t)) for t in top.getElements().elementTypes(dim=1))
    nb = sum(len(bottom.getElements()(t)) for t in bottom.getElements().elementTypes(dim=1))
    assert n == nt + nb > 0
    # a contact model's detector is not a stand-alone one
    with pytest.raises(TypeError):
        aka.ContactDetector(mesh, "a", "b")


def test_py086_standalone_detector_without_a_section_raises(tmp_path):
    (tmp_path / "empty.dat").write_text("material elastic [\n  name = x\n]\n")
    detection_inputs(tmp_path)
    aka.parseInput(str(tmp_path / "empty.dat"))
    mesh = aka.Mesh(2)
    mesh.read(str(tmp_path / "detection-explicit.msh"))
    with pytest.raises(aka.Exception, match="contact_detector"):
        aka.ContactDetector(mesh)


def test_py086_the_positions_form_is_refused(tmp_path):
    """UD-1170 (A): upstream's ``ContactDetector(mesh, positions)`` frees
    the positions buffer twice and aborts (oracle probe M11-08); the port
    refuses the form before touching the array."""
    detection_inputs(tmp_path)
    aka.parseInput(str(tmp_path / "detection-explicit.dat"))
    mesh = aka.Mesh(2)
    mesh.read(str(tmp_path / "detection-explicit.msh"))
    pos = np.array(mesh.getNodes())
    with pytest.raises(aka.Exception, match="UD-1170"):
        aka.ContactDetector(mesh, pos)
    with pytest.raises(aka.Exception, match="UD-1170"):
        aka.ContactDetector(mesh, positions=pos, id="d")
