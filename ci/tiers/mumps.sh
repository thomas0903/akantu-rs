#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-027 implicit back-end Mumps, CI-011 image).
# mumps: the MUMPS tier at 1/2/4 ranks (scripts/mumps/run-mumps-tests.sh, akantu-rs-mumps:dev)
#
# Always runs in Docker (needs MUMPS 5.7.3, ScaLAPACK, PT-Scotch and the BLAS
# pin of the image; build it with docker build -t akantu-rs-mumps:dev
# docker/mumps after scripts/mpi/build-image.sh); the backend choice does not
# apply. Extra arguments after `--` of run-all.sh go to run-mumps-tests.sh
# (e.g. --ranks "1 2" --repeat 3). Pass/fail is that script's own (its
# scenarios, the oracle comparison where the fixtures exist, clippy of the
# feature, the regression suites).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin mumps
img="${AKANTU_RS_MUMPS_IMAGE:-akantu-rs-mumps:dev}"
if [ ! -x "$CI_REPO/scripts/mumps/run-mumps-tests.sh" ]; then
  ci_na mumps "this tree has no scripts/mumps/run-mumps-tests.sh (MUMPS not merged)"
elif ! ci_have docker || ! docker image inspect "$img" > /dev/null 2>&1; then
  ci_skip mumps "Docker or image $img not available (docker build -t akantu-rs-mumps:dev docker/mumps)"
else
  ci_step mumps --host -- env AKANTU_RS_MPI_IMAGE="$img" AKANTU_RS_MPI_TARGET="/target/$CI_TARGET_NAME-mumps" "$CI_REPO/scripts/mumps/run-mumps-tests.sh" "$@"
fi
ci_tier_end
