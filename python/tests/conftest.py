# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs test support for the Python interface.
"""Shared fixtures: upstream tests run in their source directory and open
inputs by relative name; ``upstream_dir`` reproduces that with the copies
kept in this repository."""

import os
import pathlib
import sys

import pytest

HERE = pathlib.Path(__file__).resolve().parent

# No test may open a window: several example scripts call `plt.show()`, and
# matplotlib's default backend on macOS is the GUI backend `macosx` (on a
# Linux host without a display it falls back to Agg, the oracle's). The
# examples and the scripts the tests start as subprocesses inherit this
# (without it a native macOS run opens windows and blocks in `plt.show()`).
os.environ["MPLBACKEND"] = "Agg"

# `import py11_akantu_test_common` (upstream test module) resolves to the
# shim next to this file.
sys.path.insert(0, str(HERE))


@pytest.fixture
def upstream_dir(monkeypatch, tmp_path):
    """Return a function that chdirs into a scratch directory holding the
    given files (``{name: source path}``), like upstream's CMake test
    directories."""

    def make(**files):
        for name, src in files.items():
            (tmp_path / name).write_bytes(pathlib.Path(src).read_bytes())
        monkeypatch.chdir(tmp_path)
        return tmp_path

    return make
