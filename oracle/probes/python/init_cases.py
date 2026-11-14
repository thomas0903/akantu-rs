# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: drives oracle/probes/cpp/init_probe.cc under mpirun for every
# seed-policy scenario of akantu::initialize (row CORE-031).
"""Run init_probe for every scenario with 1 and 2 ranks.

  python3 -I init_cases.py <init_probe binary> <fixtures_stage_dir>

Each scenario sets the environment (AKA_SEED), the extra command-line
arguments and the input file given to `initialize(input_file, argc, argv)`;
every rank writes core/initialize/<scenario>_np<N>_r<rank>.
"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

INPUTS = {
    "seed99.dat": "seed = 99\n",
    "seed123.dat": "seed = 123\n",
    "seed456.dat": "seed = 456\n",
    "seed_float.dat": "seed = 12.7\n",
    "seed_expr.dat": "seed = 2*3\n",
    "seed_scoped.dat": "user section [\n  seed = 5\n]\n",
    "seed_negative.dat": "seed = -40\n",
}

# name -> (AKA_SEED or None, extra args, input file or None); file names are
# relative to the working directory of the run (deterministic fixtures).
SCENARIOS = {
    "env": ("4242", ["--aka_seed", "77"], "seed99.dat"),
    "env_atol": ("  -17xyz", [], None),
    "cli": (None, ["--aka_seed", "77", "keep_me"], "seed99.dat"),
    "file": (None, [], "seed99.dat"),
    "file_negative": (None, [], "seed_negative.dat"),
    "cli_input_file": (None, ["--aka_input_file", "@seed123.dat"], "seed456.dat"),
    "file_float": (None, [], "seed_float.dat"),
    "file_expr": (None, [], "seed_expr.dat"),
    "file_scoped": (None, [], "seed_scoped.dat"),
    "time": (None, [], None),
    "overflow": ("4611686018427387904", [], None),
    "debug_level": ("5", ["--aka_debug_level", "7", "--aka_print_backtrace", "x"], None),
}


def main() -> int:
    probe, stage = sys.argv[1], sys.argv[2]
    work = Path(tempfile.mkdtemp(prefix="akantu-init-cases."))
    for name, text in INPUTS.items():
        (work / name).write_text(text, encoding="utf-8")
    status = 0
    for scenario, (env_seed, args, input_file) in SCENARIOS.items():
        for np_ in (1, 2):
            env = dict(os.environ)
            env.pop("AKA_SEED", None)
            env.pop("AKA_DEBUG_LEVEL", None)
            mpi_env = []
            if env_seed is not None:
                env["AKA_SEED"] = env_seed
                mpi_env = ["-x", "AKA_SEED"]
            extra = [a[1:] if a.startswith("@") else a for a in args]
            infile = input_file if input_file else "-"
            cmd = (["mpirun", "--oversubscribe", "-np", str(np_)] + mpi_env +
                   [probe, stage, scenario, infile] + extra)
            r = subprocess.run(cmd, cwd=work, env=env, capture_output=True, text=True)
            sys.stdout.write(r.stdout)
            if r.returncode != 0:
                sys.stderr.write(f"init_probe {scenario} np={np_} failed ({r.returncode}):\n"
                                 f"{r.stderr}\n")
                status = 1
    return status


if __name__ == "__main__":
    sys.exit(main())
