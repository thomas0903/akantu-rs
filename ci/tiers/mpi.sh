#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-003 PARALLEL levels, CI-011 MPI image settings).
# mpi: akantu-comm real-MPI tier at 1/2/4 ranks (scripts/mpi/run-tests.sh, akantu-rs-mpi:dev)
#
# Always runs in Docker (the tier needs Open MPI 5.0.7 of the image); the
# backend choice does not apply. Extra arguments after `--` of run-all.sh
# go to run-tests.sh (e.g. --repeat 3). Pass/fail is run-tests.sh's own
# (every scenario, its evidence checks and the performance gate).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin mpi
img="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
if ! ci_have docker || ! docker image inspect "$img" > /dev/null 2>&1; then
  ci_skip mpi "Docker or image $img not available (build it with scripts/mpi/build-image.sh)"
else
  ci_step mpi --host -- env AKANTU_RS_MPI_IMAGE="$img" AKANTU_RS_MPI_TARGET="/target/$CI_TARGET_NAME" "$CI_REPO/scripts/mpi/run-tests.sh" "$@"
fi
ci_tier_end
