#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-003 PARALLEL levels, CI-027 parallel backends).
# par: distribution + parallel SMM on real MPI at 1/2/4 ranks with Scotch (run-par-tests.sh --scotch)
#
# Always in Docker with akantu-rs-scotch:dev (AKANTU_RS_SCOTCH_IMAGE).
# Extra arguments after `--` of run-all.sh go to run-par-tests.sh.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin par
img="${AKANTU_RS_SCOTCH_IMAGE:-akantu-rs-scotch:dev}"
if ! ci_have docker || ! docker image inspect "$img" > /dev/null 2>&1; then
  ci_skip par "Docker or image $img not available (docker build -t akantu-rs-scotch:dev docker/scotch)"
else
  ci_step par --host -- env AKANTU_RS_MPI_IMAGE="$img" AKANTU_RS_MPI_TARGET="/target/$CI_TARGET_NAME-par" "$CI_REPO/scripts/mpi/run-par-tests.sh" --scotch "$@"
fi
ci_tier_end
