# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs test support for the ported upstream Python patch tests
# (test/test_model/patch_tests/*.py, TEST-mapping in docs/FEATURE_PARITY.md).
"""Reproduce the upstream CMake test directory: the patch-test meshes
``_<type>.msh`` and the ``material_check_stress_*.dat`` files in the current
directory, the fixture modules importable."""

import pathlib
import sys

import pytest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent.parent
TYPES = [
    "segment_2", "segment_3", "triangle_3", "triangle_6", "quadrangle_4",
    "quadrangle_8", "tetrahedron_4", "tetrahedron_10", "hexahedron_8",
    "hexahedron_20", "pentahedron_6", "pentahedron_15",
]
MATERIALS = REPO / "crates/akantu-parser/tests/fixtures/test/test_model/patch_tests/data"

sys.path.insert(0, str(HERE))

# TEST-060: upstream's script (copied unchanged) is broken upstream (UD-837);
# test_patch_linear_anisotropic_explicit_upstream.py runs it as a script
collect_ignore = ["test_patch_linear_anisotropic_explicit.py"]


def _mesh(t):
    # copies of upstream test/test_model/patch_tests/data/_<type>.msh kept
    # with the SMM differential fixtures
    for p in sorted((REPO / "fixtures/model/smm").glob(f"*/inputs/_{t}.msh")):
        return p
    raise FileNotFoundError(f"_{t}.msh")


@pytest.fixture(autouse=True)
def _patch_test_dir(monkeypatch, tmp_path):
    for t in TYPES:
        (tmp_path / f"_{t}.msh").write_bytes(_mesh(t).read_bytes())
    for p in MATERIALS.glob("material_check_stress_*.dat"):
        (tmp_path / p.name).write_bytes(p.read_bytes())
    # The heat-transfer patch tests (TEST-061…063)
    p = MATERIALS / "heat_transfer_input.dat"
    (tmp_path / p.name).write_bytes(p.read_bytes())
    monkeypatch.chdir(tmp_path)
    return tmp_path


def pytest_pycollect_makeitem(collector, name, obj):
    """Upstream runs these scripts with plain ``python``; under pytest the
    fixture ``unittest.TestCase`` subclasses imported into the test modules
    are not tests (their constructor takes the element type): skip them,
    collect only the ``test()`` functions."""
    if name.startswith("TestPatchTest"):
        return []
    return None
