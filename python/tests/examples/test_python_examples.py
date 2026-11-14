# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: upstream's Python examples (examples/python/** of
# Akantu, EPFL/LSMS; unchanged copies in python/examples) run with the port
# and compared with the oracle runs committed in fixtures/python/examples
# (docs/TESTING.md; cases: python/examples/cases.py).
"""Python example comparisons.

Every case runs the port in a fresh directory through
``python/examples/harness.py`` (a subprocess: each example parses its own
input file into the global parser and registers its own laws) and compares
everything it printed, wrote and computed with the oracle fixture:
standard output byte for byte, every output file by SHA-256 (VTU files
array by array), the script's numeric variables and the captured sparse
matrices bitwise. The only comparison rules are the documented upstream
defects UD-110 (matrix arrays written as zero-filled squares), UD-114
(invalid base64 of empty arrays), UD-061 (Matrix Market files with 15
digits) and the per-case ignored keys of cases.py (UD-085). Cases blocked by
a binding gap run up to the refused call: the port must refuse exactly
there, and the state reached is compared with the oracle stopped before
the same line. Cases whose upstream run raises (``raises``) must raise the
same exception at the same line in the port; what was computed until then
(outputs, variables, the case's probes) is compared.
"""

import importlib.util
import os
import pathlib
import subprocess
import sys

import numpy as np
import pytest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]
EXAMPLES = REPO / "python/examples"
FIXTURES = REPO / "fixtures/python/examples"

_spec = importlib.util.spec_from_file_location("ex_harness", EXAMPLES / "harness.py")
harness = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(harness)
CASES = harness.load_cases()

# `raises`: upstream's own run raises; the port must raise the same
# exception at the same line, and everything before it is compared,
# including the case's probes at the exception
COMPARED = sorted(c for c, d in CASES.items()
                  if d["check"] in ("exact", "blocked", "raises"))
_cspec = importlib.util.spec_from_file_location("ex_cases", EXAMPLES / "cases.py")
_cases = importlib.util.module_from_spec(_cspec)
_cspec.loader.exec_module(_cases)
ORACLE_ENVIRONMENT = _cases.ORACLE_ENVIRONMENT


# This interpreter's values of the oracle environment's entries (cases.py:
# also printed by ci/tools/python_env.py in the wheel tier).
ENVIRONMENT = _cases.environment()


def _akantu_dir():
    """The first directory of this interpreter's path holding an `akantu`
    package with its extension (the harness runs in its own run directory,
    where the current directory of this process is not on the path)."""
    for d in [os.getcwd()] + [p for p in sys.path if p]:
        pkg = pathlib.Path(d) / "akantu"
        if (pkg / "__init__.py").exists() and any(pkg.glob("py11_akantu*")):
            return str(pkg.parent.resolve())
    return None


def run_port(case, tmp_path):
    work = tmp_path / case
    work.mkdir()
    # MPLBACKEND: `plt.show()` of the examples must not open a window or
    # block (macOS defaults to the GUI backend; conftest.py sets it as well).
    env = dict(os.environ, OPENBLAS_CORETYPE="Haswell", OPENBLAS_NUM_THREADS="1",
               OMP_NUM_THREADS="1", MPLBACKEND="Agg")
    where = _akantu_dir()
    if where is not None:
        env["PYTHONPATH"] = os.pathsep.join(
            [where] + ([env["PYTHONPATH"]] if env.get("PYTHONPATH") else []))
    with open(work / "stdout.txt", "wb") as out:
        r = subprocess.run(
            [sys.executable, "-u", str(EXAMPLES / "harness.py"), "run", case,
             str(work), "--side", "port"],
            stdout=out, stderr=subprocess.PIPE, env=env, check=False)
    return work, r


@pytest.mark.parametrize("case", COMPARED)
def test_example_matches_the_oracle(case, tmp_path):
    d = CASES[case]
    differs = [k for k in d["env_skip"] if ENVIRONMENT[k] != ORACLE_ENVIRONMENT[k]]
    if differs:
        # the script's own result depends on these (upstream's too): the
        # comparison is made in the oracle image (docs/TESTING.md)
        pytest.skip(f"{case} depends on {differs}: here "
                    f"{[ENVIRONMENT[k] for k in differs]}, in the oracle runs "
                    f"{[ORACLE_ENVIRONMENT[k] for k in differs]}")
    ignore = dict(d["ignore"])
    for k, keys in d["env_ignore"].items():
        if ENVIRONMENT[k] != ORACLE_ENVIRONMENT[k]:
            ignore.update({key: f"depends on {k}" for key in keys})
    work, r = run_port(case, tmp_path)
    status = (work / "status.txt").read_text() if (work / "status.txt").exists() else ""
    assert r.returncode == 0, f"{status}\n{r.stderr.decode(errors='replace')[-3000:]}"
    fixture = FIXTURES / (d["oracle"] or case)
    report, notes = harness.compare(fixture, work, ignore=ignore,
                                    stdout_mask=d["stdout_mask"])
    assert report == [], "\n".join(report + ["notes:"] + notes)
    if d["check"] == "blocked":
        # the port refused the call of the stop line, the oracle stopped there
        assert f"at line {d['stop_line']}:" in status, status
        if d["refusal"] is not None:
            # which refusal
            exc, fragment = d["refusal"]
            assert f"status raised {exc} at line {d['stop_line']}:" in status, status
            assert fragment in status, status


def test_examples_cannot_open_a_window():
    """A script started as the example cases start theirs (and as
    the other tests start their subprocesses: os.environ) gets matplotlib's
    non-interactive Agg backend, so `plt.show()` neither opens a window nor
    blocks. On macOS the default backend is the GUI backend `macosx`."""
    assert os.environ.get("MPLBACKEND") == "Agg"  # conftest.py
    if importlib.util.find_spec("matplotlib") is None:
        return  # nothing to start; the variable is what the examples inherit
    probe = ("import matplotlib, matplotlib.pyplot as plt; "
             "print(matplotlib.get_backend().lower()); plt.plot([0, 1]); plt.show()")
    r = subprocess.run([sys.executable, "-c", probe], capture_output=True,
                       text=True, env=dict(os.environ), timeout=120, check=False)
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "agg", r.stdout


def test_case_table_covers_every_upstream_script():
    """Every upstream example script is run by a case (image_saver.py is a
    helper imported by eigen_modes.py)."""
    scripts = {p.relative_to(EXAMPLES).as_posix()
               for p in EXAMPLES.rglob("*.py")
               if "variants" not in p.parts and "oracle" not in p.parts
               and p.parent != EXAMPLES}
    used = {d["script"] for d in CASES.values()}
    assert scripts - used == {"solid_mechanics_model/eigen_modes/image_saver.py"}
    for c, d in CASES.items():
        # an `independent` case is checked against the oracle run it
        # departs from by design (its `oracle`), it has no fixture of its own
        f = d["oracle"] if d["check"] == "independent" else c
        assert (FIXTURES / f / "digests.txt").exists(), f"no oracle fixture for {c}"


# The port's default mode of EX-043 / EX-045 (the
# corrected contact of D-078 section 6: UD-363, UD-365, ...): the number of
# leading dumps equal to upstream's run, i.e. before the first contact
# (measured: compression differs from the third of 201 VTU
# files on, bloc_friction from the 22nd of 301)
DEFAULT_MODE_EQUAL_DUMPS = {"compression_default": 2, "bloc_friction_default": 21}


@pytest.mark.parametrize("case", sorted(DEFAULT_MODE_EQUAL_DUMPS))
def test_contact_examples_default_mode(case, tmp_path):
    """The unmodified script with the port's corrected contact defaults
    runs to its end; every dump before the first contact equals upstream's
    (the compat-mode run equals all of them, `test_example_matches_the_oracle`),
    the later ones differ by design."""
    d = CASES[case]
    work, r = run_port(case, tmp_path)
    status = (work / "status.txt").read_text() if (work / "status.txt").exists() else ""
    assert r.returncode == 0, f"{status}\n{r.stderr.decode(errors='replace')[-3000:]}"
    assert "status completed" in status, status
    fixture = FIXTURES / d["oracle"]
    f_vtu = {}
    for line in (fixture / "vtu_files.txt").read_text().splitlines():
        f, _, digest, _ = line.split("\t")
        f_vtu[f] = digest
    got = harness.summary(work)[2]
    assert sorted(got) == sorted(f_vtu)
    differ = [f for f in sorted(f_vtu) if got[f][0] != f_vtu[f]]
    equal_before = DEFAULT_MODE_EQUAL_DUMPS[case]
    assert differ, "the corrected contact changes the run after the first contact"
    assert differ == sorted(f_vtu)[equal_before:], (differ[:3], len(differ))


def _damage_dumps(work):
    """Nodal damage of every dump of the phase-field run, in dump order."""
    files = sorted((work / "run/paraview/phasefield-static-VTU").glob("*.vtu"))
    return [harness._vtu_values(f, "damage") for f in files]


def test_phasefield_static_corrected_default_independent_checks(tmp_path):
    """The unmodified script with the port's corrected defaults (UD-387:
    phase-field driving energy, inactive for this law: no energy split):
    besides the oracle comparison above, independent checks of the run:
    the damage stays in [0, 1] at every node of every dump and never
    decreases between dumps (history field), it localises (the largest
    nodal damage of the last dump exceeds the first dump's), and the
    staggered loop converged at every load step (the script raises
    otherwise)."""
    work, r = run_port("phasefield_static", tmp_path)
    assert r.returncode == 0, r.stderr.decode(errors="replace")[-3000:]
    dumps = _damage_dumps(work)
    assert len(dumps) == 31  # every 50 steps of 1500, and the final dump
    for k, d in enumerate(dumps):
        assert np.all(d >= 0.0) and np.all(d <= 1.0), f"dump {k}: {d.min()} {d.max()}"
    for k in range(1, len(dumps)):
        assert np.all(dumps[k] >= dumps[k - 1]), f"damage decreased before dump {k}"
    assert dumps[-1].max() > dumps[0].max()
