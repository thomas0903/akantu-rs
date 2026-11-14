#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-024: Sphinx + breathe + RTD).
# pydocs: HTML API documentation of the `akantu` Python package (pdoc over the installed wheel); nothing is hosted
#
# Upstream builds its documentation with Sphinx (breathe over Doxygen XML for
# C++, autodoc for Python) and hosts it on Read the Docs. The C++ part is
# `cargo doc` (tier `docs`); this tier is the Python part: it installs the
# wheel built by the `wheel` tier (<out>/wheel/wheels/*.whl; run `--tiers
# wheel,pydocs`, i.e. the `python` preset) into a throw-away venv together
# with pdoc (CI_PDOC_VERSION, default 16.0.0, pinned; network) and writes
# <tier dir>/html with `python -m pdoc akantu -o html`. pdoc documents what
# the module exports at import time: it fails on an import error, and the
# check below fails when the output holds no page for the package or the page does not
# document the model classes re-exported from the compiled module. pdoc
# warns about names in `akantu.__all__` that do not resolve (the Diffusion /
# PhaseField constitutive-law handler names): reported in docs/TESTING.md,
# not hidden. The `python` workflow keeps the HTML as an artifact; the
# `site` job of docs.yml publishes it with the manual on GitHub Pages
# (ci/github/README.md).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin pydocs
py="${CI_WHEEL_PYTHON:-python3}"
wheels="$CI_OUT/wheel/wheels"
if [ ! -f "$CI_REPO/python/pyproject.toml" ]; then
  ci_na pdoc "no python/pyproject.toml on this tree (no Python package)"
elif ! ls "$wheels"/*.whl > /dev/null 2>&1 && [ "$CI_DRY_RUN" != 1 ]; then
  ci_skip pdoc "no wheel in $wheels (run the wheel tier first: --tiers wheel,pydocs, or --preset python)"
elif ! "$py" -c 'import ensurepip, venv' > /dev/null 2>&1; then
  ci_skip pdoc "the host needs $py with venv/ensurepip"
else
  html="$CI_TIER_DIR/html"
  venv="$CI_TIER_DIR/venv"
  # shellcheck disable=SC2016
  ci_step pdoc --host -- sh -c '
    set -e
    rm -rf "$2" "$3"; "$1" -m venv "$2"
    "$2/bin/pip" install -q "pdoc==$5" "$4"/*.whl
    cd /
    PYTHONDONTWRITEBYTECODE=1 "$2/bin/python" -m pdoc akantu -o "$3"
    find "$3" -name "*.html" | sort
  ' sh "$py" "$venv" "$html" "$wheels" "${CI_PDOC_VERSION:-16.0.0}"
  # shellcheck disable=SC2016
  ci_step html-present --host -- sh -c '
    test -s "$1/akantu.html" || { echo "no akantu.html in $1"; exit 1; }
    grep -q "SolidMechanicsModel" "$1/akantu.html" || { echo "akantu.html does not document SolidMechanicsModel: the names of the compiled module did not reach the page"; exit 1; }
    n="$(find "$1" -name "*.html" | wc -l)"; echo "$n HTML pages"
  ' sh "$html"
  rm -rf "$venv"
fi
ci_tier_end
