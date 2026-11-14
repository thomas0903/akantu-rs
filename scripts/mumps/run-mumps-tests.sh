#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Host entry point of the MUMPS tier (docs/TESTING.md): runs
# scripts/mumps/run-mumps-tier.sh inside the akantu-rs-mumps image (build it
# first: docker build -t akantu-rs-mumps:dev docker/mumps, which needs
# akantu-rs-mpi:dev from scripts/mpi/build-image.sh).
#
#   scripts/mumps/run-mumps-tests.sh [--ranks "1 2 4"] [--repeat N] [--skip-lint]
#   AKANTU_RS_MPI_TARGET=/target/<name> scripts/mumps/run-mumps-tests.sh  # own target dir
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
dirty=""
if ! git -C "$here" diff --quiet HEAD 2>/dev/null; then dirty="-dirty"; fi
export AKANTU_RS_MPI_IMAGE="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mumps:dev}"
AKANTU_RS_GIT_HEAD="$(git -C "$here" rev-parse HEAD 2>/dev/null || echo unknown)$dirty"
AKANTU_RS_MPI_IMAGE_ID="$(docker image inspect "$AKANTU_RS_MPI_IMAGE" --format '{{.Id}}' 2>/dev/null || echo unknown)"
export AKANTU_RS_GIT_HEAD AKANTU_RS_MPI_IMAGE_ID
export AKANTU_RS_MPI_TARGET="${AKANTU_RS_MPI_TARGET:-/target/mumps}"
exec "$here/scripts/mpi/in-docker.sh" bash /src/scripts/mumps/run-mumps-tier.sh "$@"
