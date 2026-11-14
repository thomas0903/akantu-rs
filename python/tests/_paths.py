# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs test support: repository locations of the upstream
# test inputs used by the Python tests.
"""Paths of the repository copies of upstream test inputs."""

import pathlib

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
PARSER_FIXTURES = REPO / "crates/akantu-parser/tests/fixtures"
MESHES = REPO / "fixtures/meshes/gmsh-4.13.1"
