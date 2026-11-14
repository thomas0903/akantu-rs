# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs test (TEST-060): runs upstream's
# test/test_model/patch_tests/test_patch_linear_anisotropic_explicit.py
# (copied unchanged next to this file) as upstream's CMake test does
# (register_test(test_linear_anisotropic_explicit_python, UNSTABLE,
# FILES_TO_COPY the two fixtures and data/material_anisotropic_3.dat)).
"""TEST-060: upstream's anisotropic Python patch test is broken (UD-837): it
parses ``material_anisotropic.dat`` while its registration copies
``material_anisotropic_3.dat``, so it stops at the first element type with an
``akantu.Exception`` (the pinned oracle: exit status 1, same traceback).
Beyond that point it calls methods the fixture does not define
(``checkDisplacements``, ``checkStrains``) and ``strain(k, l)`` on an ndarray.
The port reproduces the upstream outcome (D-003 category C: a quirk
preserved)."""

import os
import pathlib
import shutil
import subprocess
import sys

import akantu as aka

HERE = pathlib.Path(__file__).resolve().parent
DATA = (HERE.parent.parent.parent
        / "crates/akantu-parser/tests/fixtures/test/test_model/patch_tests/data")


def test_upstream_script_outcome(_patch_test_dir):
    work = _patch_test_dir
    for f in ("test_patch_linear_anisotropic_explicit.py",
              "patch_test_linear_fixture.py",
              "patch_test_linear_solid_mechanics_fixture.py"):
        shutil.copy(HERE / f, work / f)
    shutil.copy(DATA / "material_anisotropic_3.dat", work)
    env = dict(os.environ)
    # the akantu package this test imported (installed wheel or in-tree)
    pkg = pathlib.Path(aka.__file__).resolve().parent.parent
    env["PYTHONPATH"] = os.pathsep.join([str(pkg), env.get("PYTHONPATH", "")])
    r = subprocess.run(
        [sys.executable, "test_patch_linear_anisotropic_explicit.py"],
        cwd=work, env=env, capture_output=True, text=True, timeout=600,
    )
    assert r.returncode == 1, r.stdout + r.stderr
    assert 'akantu.parseInput(material_file)' in r.stderr
    assert "akantu.py11_akantu.Exception" in r.stderr
    assert "material_anisotropic.dat" in r.stderr.splitlines()[-1]
