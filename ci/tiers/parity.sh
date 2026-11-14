#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (project bookkeeping; rows CI-001, INT).
# parity: the STATS block of docs/FEATURE_PARITY.md and the generated status block of README.md equal scripts/parity/stats.py --update
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin parity

if [ ! -f "$CI_REPO/scripts/parity/stats.py" ] || [ ! -f "$CI_REPO/docs/FEATURE_PARITY.md" ]; then
  ci_na stats "no scripts/parity/stats.py or docs/FEATURE_PARITY.md"
elif ! ci_have python3; then
  ci_skip stats "python3 not installed on the host"
else
  # stats.py --update rewrites docs/FEATURE_PARITY.md AND README.md (the
  # status block checked by crates/akantu/tests/manual.rs::
  # readme_status_matches_parity_stats) relative to its own location: run it
  # on a scratch copy of the three files and compare both (the repository is
  # never modified; a scratch tree without README.md made the
  # tier crash after the merge of main).
  scratch="$(ci_file scratch)"
  rm -rf "$scratch" && mkdir -p "$scratch/scripts/parity" "$scratch/docs"
  cp "$CI_REPO/scripts/parity/stats.py" "$scratch/scripts/parity/"
  cp "$CI_REPO/docs/FEATURE_PARITY.md" "$scratch/docs/"
  if [ -f "$CI_REPO/README.md" ]; then cp "$CI_REPO/README.md" "$scratch/"; fi
  # shellcheck disable=SC2016
  ci_step stats --host -- sh -c 'python3 -I "$1/scripts/parity/stats.py" --update > /dev/null && diff -u "$2/docs/FEATURE_PARITY.md" "$1/docs/FEATURE_PARITY.md" && { [ ! -f "$2/README.md" ] || diff -u "$2/README.md" "$1/README.md"; } && echo "STATS block and README status block up to date"' sh "$scratch" "$CI_REPO"
fi

ci_tier_end
