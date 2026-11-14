#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-024: Sphinx/Doxygen/RTD → rustdoc).
# docs: cargo doc --workspace --no-deps with RUSTDOCFLAGS="-D warnings" (+ doc tests run in `test`)
#
# API documentation of every member (the Doxygen/breathe part of upstream's
# documentation build). Broken intra-doc links and other rustdoc warnings
# fail the tier, like clippy warnings; --keep-going documents every crate
# so that one run lists all of them. The Python API documentation (Sphinx
# over the bindings) is replaced by the `pydocs` tier (pdoc over the wheel).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin docs
if ! ci_backend_ready; then
  ci_skip rustdoc "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
else
  ci_step rustdoc -- env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --keep-going
fi
if [ -f "$CI_REPO/python/pyproject.toml" ] && [ -d "$CI_REPO/doc" ]; then
  ci_skip sphinx "Python API docs: no Sphinx job defined yet (DOC rows)"
else
  ci_na sphinx "no Python package documentation on this tree (DOC rows)"
fi
ci_tier_end
