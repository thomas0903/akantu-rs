#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-003: PARALLEL_LEVEL N…1 → fixed 8 ranks).
# expensive: 8-rank MPI and parallel-SMM tiers (run-tests.sh / run-par-tests.sh --ranks 8)
#
# The expensive tier of docs/TESTING.md that has a script today. Benchmarks
# and the full-size cohesive run are not part of it (docs/BENCHMARKS.md).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin expensive
mpi_img="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
sc_img="${AKANTU_RS_SCOTCH_IMAGE:-akantu-rs-scotch:dev}"
if ! ci_have docker || ! docker image inspect "$mpi_img" > /dev/null 2>&1; then
  ci_skip mpi-8 "Docker or image $mpi_img not available"
else
  ci_step mpi-8 --host -- env AKANTU_RS_MPI_IMAGE="$mpi_img" AKANTU_RS_MPI_TARGET="/target/$CI_TARGET_NAME" "$CI_REPO/scripts/mpi/run-tests.sh" --ranks 8 "$@"
fi
if ! ci_have docker || ! docker image inspect "$sc_img" > /dev/null 2>&1; then
  ci_skip par-8 "Docker or image $sc_img not available"
else
  ci_step par-8 --host -- env AKANTU_RS_MPI_IMAGE="$sc_img" AKANTU_RS_MPI_TARGET="/target/$CI_TARGET_NAME-par" "$CI_REPO/scripts/mpi/run-par-tests.sh" --scotch --ranks 8 "$@"
fi
ci_tier_end
