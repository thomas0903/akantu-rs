#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): what a
# CI run must print so that the Python example comparisons of its log can be
# judged: the CPU, NumPy's CPU features and dispatch targets, and the values
# of the oracle environment's entries that python/tests/examples compares
# (python/examples/cases.py: `environment()` against `ORACLE_ENVIRONMENT`).
"""Print the Python environment the example comparisons see.

    python_env.py CASES_PY

Run with the interpreter of the tests (the wheel tier's venv). Prints the CPU
model, ``numpy.show_runtime()`` and one line per entry of
``ORACLE_ENVIRONMENT`` (``same`` / ``DIFFERS``); the cases whose
``env_skip`` names a differing entry skip, those whose ``env_ignore`` names
one leave out its keys. Exit 0 always once NumPy imports (the verdict is the
pytest step's), 2 on a usage error.
"""

from __future__ import annotations

import importlib.util
import platform
import subprocess
import sys
from pathlib import Path


def cpu_model() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    if sys.platform == "darwin":
        try:
            r = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"],
                               capture_output=True, text=True, check=False)
            if r.stdout.strip():
                return r.stdout.strip()
        except OSError:
            pass
    return platform.processor() or "unknown"


def cpu_flag(flag: str) -> bool | None:
    """Whether the kernel lists `flag` for the CPU (/proc/cpuinfo, Linux);
    None where there is no such list."""
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("flags"):
                return flag in line.split(":", 1)[1].split()
    except OSError:
        pass
    return None


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__)
        return 2
    spec = importlib.util.spec_from_file_location("ex_cases", argv[0])
    cases = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cases)
    import numpy as np

    print(f"cpu: {cpu_model()}")
    print(f"python: {sys.version.split()[0]} ({sys.executable})")
    sys.stdout.flush()
    np.show_runtime()
    sys.stdout.flush()
    env = cases.environment()
    oracle = cases.ORACLE_ENVIRONMENT
    for key in sorted(oracle):
        mark = "same" if env.get(key) == oracle[key] else "DIFFERS"
        print(f"environment {key}: here {env.get(key)!r}, oracle {oracle[key]!r}: {mark}")
    # An independent reading of the CPU feature that `numpy_avx512f` stands
    # for: a disagreement on a run without NPY_DISABLE_CPU_FEATURES /
    # NPY_ENABLE_CPU_FEATURES means NumPy's introspection, not the CPU, made
    # the two pulse cases skip.
    flag = cpu_flag("avx512f")
    if flag is not None:
        agree = flag == (env.get("numpy_avx512f") is True)
        print(f"cpu flag avx512f (/proc/cpuinfo): {flag}; numpy_avx512f: "
              f"{env.get('numpy_avx512f')!r}: {'agrees' if agree else 'DISAGREES'}")
    for name, d in sorted(cases.CASES.items()):
        skip = [k for k in d["env_skip"] if env.get(k) != oracle[k]]
        ign = [k for k in d["env_ignore"] if env.get(k) != oracle[k]]
        if skip:
            print(f"case {name}: skips here (depends on {skip})")
        elif ign:
            print(f"case {name}: compared without {[x for k in ign for x in d['env_ignore'][k]]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
