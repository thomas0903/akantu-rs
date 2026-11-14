#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Host entry point of the parallel MPI tier (docs/TESTING.md): runs scripts/mpi/run-par-tier.sh inside
# the akantu-rs-mpi image (or AKANTU_RS_MPI_IMAGE, e.g. the Scotch image of
# docker/scotch for --scotch).
#
#   scripts/mpi/run-par-tests.sh                   # 1/2/4 ranks
#   scripts/mpi/run-par-tests.sh --repeat 3        # repeatability
#   scripts/mpi/run-par-tests.sh --ranks "8"       # expensive tier
#   AKANTU_RS_MPI_TARGET=/target/<name> scripts/mpi/run-par-tests.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
dirty=""
if ! git -C "$here" diff --quiet HEAD 2>/dev/null; then dirty="-dirty"; fi
AKANTU_RS_GIT_HEAD="$(git -C "$here" rev-parse HEAD 2>/dev/null || echo unknown)$dirty"
AKANTU_RS_MPI_IMAGE_ID="$(docker image inspect "${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}" --format '{{.Id}}' 2>/dev/null || echo unknown)"
export AKANTU_RS_GIT_HEAD AKANTU_RS_MPI_IMAGE_ID
exec "$here/scripts/mpi/in-docker.sh" bash /src/scripts/mpi/run-par-tier.sh "$@"
