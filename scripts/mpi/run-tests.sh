#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Host entry point of the akantu-comm MPI tier (docs/TESTING.md):
# runs scripts/mpi/run-mpi-tier.sh inside the akantu-rs-mpi image
# (build it first with scripts/mpi/build-image.sh).
#
#   scripts/mpi/run-tests.sh                      # 1/2/4 ranks (medium tier)
#   scripts/mpi/run-tests.sh --ranks "8"          # expensive tier (document only)
#   scripts/mpi/run-tests.sh --repeat 3           # repeatability
#   scripts/mpi/run-tests.sh --ranks "8" --repeat-abort 200  # teardown-race rate
#   AKANTU_RS_MPI_TARGET=/target/<name> scripts/mpi/run-tests.sh  # own target dir
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
# Provenance recorded by the tier (provenance.txt next to the logs).
dirty=""
if ! git -C "$here" diff --quiet HEAD 2>/dev/null; then dirty="-dirty"; fi
AKANTU_RS_GIT_HEAD="$(git -C "$here" rev-parse HEAD 2>/dev/null || echo unknown)$dirty"
AKANTU_RS_MPI_IMAGE_ID="$(docker image inspect "${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}" --format '{{.Id}}' 2>/dev/null || echo unknown)"
export AKANTU_RS_GIT_HEAD AKANTU_RS_MPI_IMAGE_ID
exec "$here/scripts/mpi/in-docker.sh" bash /src/scripts/mpi/run-mpi-tier.sh "$@"
