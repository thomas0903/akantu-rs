#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-026, CI-029; D-001, D-030, D-042).
# licence: cargo-deny-style licence audit + D-042 LGPL/GPL dependency-graph separation
#
# Runs ci/tools/licence_audit.py with ci/licence-policy.toml where cargo
# runs (host, or inside the image with the docker backend: the image has
# Python 3.13); the JSON report is report.json in the tier directory.
# CI_CARGO_OFFLINE=1 adds --offline to every cargo call.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin licence

offline=()
[ "${CI_CARGO_OFFLINE:-0}" = 1 ] && offline=(--offline)
if ! ci_backend_ready; then
  ci_skip audit "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
elif [ "$CI_BACKEND" = host ] && ! python3 -c 'import tomllib' > /dev/null 2>&1; then
  ci_skip audit "the audit needs python3 >= 3.11 (tomllib) on the host"
else
  # Paths are relative to the repository: the same words work on the host
  # and in the container (/src).
  ci_step audit --stdout "$(ci_file report.json)" -- python3 -I -B ci/tools/licence_audit.py --repo . --policy ci/licence-policy.toml ${offline[@]+"${offline[@]}"}
fi

ci_tier_end
