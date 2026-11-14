#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-013, CI-014, CI-021: scikit-build/cibuildwheel → maturin abi3).
# wheel: maturin abi3 wheel of akantu-python + `import akantu` smoke test + the whole pytest suite (no upload)
#
# Applies to trees with the Python package layout (merged:
# python/pyproject.toml with [tool.maturin]); on a tree without it (cut
# before that merge) it is skipped as incomplete, never a pass. The build runs
# on the host (any backend: the project image has no pip/ensurepip), in a
# throw-away virtual environment of the tier directory: `pip install
# maturin==1.16.0` (CI_MATURIN_VERSION; network), `maturin build --release`
# from python/ (abi3 for CPython >= 3.9 per the workspace pyo3 features), then
# `pip install` of the wheel (NumPy and SciPy come with it) and
#   * the upstream minimal acceptance check `python -c "import akantu"`
#     (CI-021; the `dynamics.py` example needs gmsh and the example ports);
#   * the step `environment` (ci/tools/python_env.py: CPU, NumPy runtime and
#     the example comparisons' environment keys);
#   * `pytest -v tests` of python/tests twice (docs/TESTING.md): with greenlet installed, and with a
#     `greenlet.py` that raises ModuleNotFoundError first on PYTHONPATH
#     (pytest.importorskip skips on ModuleNotFoundError only, not on a plain
#     ImportError: the greenlet-dependent cases then skip); both must exit 0 (pytest exits 5
#     on an empty selection, so a vanished suite fails); pytest runs from
#     python/ through its console script, so `import akantu` finds the
#     installed wheel, never the source directory. Each test's line carries
#     its duration as it finishes (`-v -o console_output_style=times`,
#     unbuffered) and the 30 slowest are listed at the end: a run cut off by
#     the job's time limit still shows where its time went (the first macOS
#     run was cancelled with dots only).
# Both pytest runs set MPLBACKEND=Agg (python/tests/conftest.py and the
# example harness set it too): several examples call `plt.show()`, which on
# macOS (default backend `macosx`) opens a window and blocks until it is
# closed; on a display-less Linux host matplotlib already falls back to Agg.
# The wheel stays in <tier dir>/wheels (the `pydocs` tier and workflow
# artifacts use it).
#
# Environment (all optional; the proposed workflows set them):
#   CI_WHEEL_PYTHON  interpreter that creates the venv (default python3)
#   CI_WHEEL_COMPAT  maturin --compatibility: `linux` (default: tagged for
#                    this machine), `manylinux_2_28` (the distributable
#                    wheel, built in the PyPA image), `off` (maturin's own
#                    default: macOS)
#   CI_WHEEL_SMOKE   further interpreters (blank-separated, e.g. the oldest
#                    CPython of the manylinux image) that install the wheel in
#                    their own venv and run `import akantu`
#   CI_WHEEL_PYTEST  0 skips the pytest runs (a skip makes the tier incomplete)
#   CI_WHEEL_PIP_BASE  pip requirement words installed BEFORE the wheel in the
#                    main venv (e.g. `numpy==2.2.4 scipy==1.15.3`: the oracle's
#                    NumPy / SciPy, so that the oracle comparisons that depend
#                    on them run instead of skipping); default none
#   CI_WHEEL_PIP_TEST  pip requirement words of the pytest environment
#                    (default pinned: pytest, greenlet and matplotlib, which
#                    the example comparisons `structural_mechanics_dynamics*`
#                    need: without it `final.times` is not defined)
# The skips of each pytest run are part of the verdict (ci/pytest-skips.txt,
# ci/tools/pytest_skips.py): a new skip fails the tier.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin wheel

py="${CI_WHEEL_PYTHON:-python3}"
compat="${CI_WHEEL_COMPAT:-linux}"
if [ ! -f "$CI_REPO/python/pyproject.toml" ]; then
  ci_skip wheel "this tree has no python/pyproject.toml (a tree cut before the Python package layout has none)"
elif ! ci_have cargo || ! "$py" -c 'import ensurepip, venv' > /dev/null 2>&1; then
  ci_skip wheel "the host needs cargo and $py with venv/ensurepip"
else
  wheels="$CI_TIER_DIR/wheels"
  venv="$CI_TIER_DIR/venv"
  pip_test="${CI_WHEEL_PIP_TEST:-pytest==8.4.2 greenlet==3.2.4 matplotlib==3.10.7}"
  case "$compat" in
    off) compat_args="" ;;
    *) compat_args="--compatibility $compat" ;;
  esac
  # shellcheck disable=SC2016
  ci_step build-and-import --host -- sh -c '
    set -e
    rm -rf "$2" "$3"; "$1" -m venv "$2"
    "$2/bin/pip" install -q "maturin==$4"
    if [ -n "$6" ]; then "$2/bin/pip" install -q $6; fi
    "$2/bin/maturin" --version
    cd python && "$2/bin/maturin" build --release --locked $5 --out "$3"
    ls -l "$3"
    "$2/bin/pip" install -q "$3"/*.whl
    cd / && "$2/bin/python" -c "import akantu, sys; print(\"import akantu OK:\", getattr(akantu, \"__version__\", \"?\"), sys.version)"
  ' sh "$py" "$venv" "$wheels" "${CI_MATURIN_VERSION:-1.16.0}" "$compat_args" "${CI_WHEEL_PIP_BASE:-}"
  for sp in ${CI_WHEEL_SMOKE:-}; do
    # shellcheck disable=SC2016
    ci_step "smoke:$(basename "$(dirname "$(dirname "$sp")")")" --host -- sh -c '
      set -e
      d="$(mktemp -d)"
      "$1" -m venv "$d/v"
      "$d/v/bin/pip" install -q "$2"/*.whl
      cd / && "$d/v/bin/python" -c "import akantu, sys; print(\"import akantu OK:\", getattr(akantu, \"__version__\", \"?\"), sys.version)"
      rm -rf "$d"
    ' sh "$sp" "$wheels"
  done
  if [ "${CI_WHEEL_PYTEST:-1}" = 0 ]; then
    ci_skip pytest "CI_WHEEL_PYTEST=0"
  elif [ "$(ci_step_status build-and-import)" != pass ] && [ "$CI_DRY_RUN" != 1 ]; then
    ci_skip pytest "the wheel did not build or import"
  else
    # What the example comparisons depend on: the CPU,
    # NumPy's CPU features and dispatch targets (numpy.show_runtime()) and the
    # entries of python/examples/cases.py ORACLE_ENVIRONMENT here, with the
    # cases that skip or leave out keys because of them. Informational: the
    # verdict is pytest's. The test requirements are installed first so that
    # `matplotlib` is reported as the tests will see it.
    # shellcheck disable=SC2016
    ci_step environment --host -- sh -c '
      set -e
      "$1/bin/pip" install -q $2
      "$1/bin/python" -I -B "$3" "$4"
    ' sh "$venv" "$pip_test" "$here/tools/python_env.py" "$CI_REPO/python/examples/cases.py"
    # shellcheck disable=SC2016
    ci_step pytest-greenlet --host -- sh -c '
      set -e
      "$1/bin/pip" install -q $2
      cd python
      MPLBACKEND=Agg PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 "$1/bin/pytest" -v -o console_output_style=times --durations=30 -p no:cacheprovider -rs tests
    ' sh "$venv" "$pip_test"
    ci_step skips-greenlet --host -- python3 -I "$here/tools/pytest_skips.py" "$(ci_file pytest-greenlet.log)" --expected "$here/pytest-skips.txt" --run greenlet
    # shellcheck disable=SC2016
    ci_step pytest-no-greenlet --host -- sh -c '
      set -e
      s="$(mktemp -d)"
      printf "raise ModuleNotFoundError(\"No module named greenlet\", name=\"greenlet\")\n" > "$s/greenlet.py"
      cd python
      PYTHONPATH="$s" MPLBACKEND=Agg PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 "$1/bin/pytest" -v -o console_output_style=times --durations=30 -p no:cacheprovider -rs tests
      rm -rf "$s"
    ' sh "$venv"
    ci_step skips-no-greenlet --host -- python3 -I "$here/tools/pytest_skips.py" "$(ci_file pytest-no-greenlet.log)" --expected "$here/pytest-skips.txt" --run no-greenlet
  fi
  # keep only the wheels (the venv is large and cheap to rebuild)
  rm -rf "$venv"
fi

ci_tier_end
