#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Host entry point of the PETSc tier (docs/TESTING.md):
# runs scripts/petsc/run-petsc-tier.sh inside the akantu-rs-petsc image
# (build it first with scripts/petsc/build-image.sh; amd64 host).
#
#   scripts/petsc/run-petsc-tests.sh                       # 1/2/4 ranks, funneled + serialized
#   scripts/petsc/run-petsc-tests.sh --ranks "8"           # expensive tier
#   AKANTU_RS_MPI_TARGET=/target/<name> scripts/petsc/run-petsc-tests.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
dirty=""
if ! git -C "$here" diff --quiet HEAD 2>/dev/null; then dirty="-dirty"; fi
AKANTU_RS_GIT_HEAD="$(git -C "$here" rev-parse HEAD 2>/dev/null || echo unknown)$dirty"
export AKANTU_RS_MPI_IMAGE="${AKANTU_RS_MPI_IMAGE:-akantu-rs-petsc:dev}"
AKANTU_RS_MPI_IMAGE_ID="$(docker image inspect "$AKANTU_RS_MPI_IMAGE" --format '{{.Id}}' 2>/dev/null || echo unknown)"
export AKANTU_RS_GIT_HEAD AKANTU_RS_MPI_IMAGE_ID
exec "$here/scripts/mpi/in-docker.sh" bash /src/scripts/petsc/run-petsc-tier.sh "$@"
