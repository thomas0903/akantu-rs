#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-017: version_determination, cmake/semver.py).
# version: tag = workspace.package.version, every member on that version, git describe printed
#
# Upstream derives the version from `git describe` (cmake/semver.py). The
# port's version is the static `workspace.package.version` of Cargo.toml (D-028
# exception proposed for the git-describe part, ci/MAPPING.md CI-017); what a
# release needs is that the tag names that version. ci/tools/version_check.py
# reads `cargo metadata`: all workspace members carry one version, the Python
# package takes it from Cargo (python/pyproject.toml `dynamic = ["version"]`),
# and when a tag is given (CI_VERSION_TAG, or GITHUB_REF_NAME when
# GITHUB_REF_TYPE=tag) it must be `v<version>` (or `<version>`). The output of
# `git describe --tags --always --dirty` is printed and recorded (a shallow
# clone prints the abbreviated commit). Without a tag the tier checks the
# members only and says so.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin version
if ! ci_backend_ready; then
  ci_skip version "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
elif ! ci_have python3; then
  ci_skip version "python3 is needed on the host for ci/tools/version_check.py"
else
  tag="${CI_VERSION_TAG:-}"
  if [ -z "$tag" ] && [ "${GITHUB_REF_TYPE:-}" = tag ]; then tag="${GITHUB_REF_NAME:-}"; fi
  meta="$(ci_file metadata.json)"
  ci_step metadata --stdout "$meta" -- cargo metadata --no-deps --format-version 1 --locked
  ci_step describe --host --stdout "$(ci_file describe.txt)" -- sh -c 'git describe --tags --always --dirty 2>/dev/null || git rev-parse --short HEAD 2>/dev/null || echo unknown'
  if [ "$CI_DRY_RUN" != 1 ]; then
    ci_step check --host -- python3 -I "$here/tools/version_check.py" "$meta" --pyproject "$CI_REPO/python/pyproject.toml" --describe "$(ci_file describe.txt)" ${tag:+--tag "$tag"}
  else
    ci_skip check "dry run"
  fi
fi
ci_tier_end
