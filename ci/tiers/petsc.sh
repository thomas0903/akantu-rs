#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-027: implicit back-end PETSc).
# petsc: the PETSc tier (scripts/petsc/run-petsc-tests.sh in akantu-rs-petsc:dev, 1/2/4 ranks x thread levels)
#
# Always Docker. The image docker/petsc is built FROM akantu-rs-mpi:dev; a
# workflow builds it in the job (`ci/github/images.sh <cache> mpi petsc`, plain
# docker build: no registry, nothing published), so the tier needs no
# prebuilt image. Extra arguments after `--` of run-all.sh go to run-petsc-tests.sh
# (e.g. --ranks "1 2"). Pass/fail is that script's own (fmt, clippy of the
# feature, cargo test, the petsc_mpi acceptance binary at every rank count and
# thread level, the oracle comparison where its fixtures exist, the BLAS-pin
# refusal). Debian's PETSc links GPL-2+ libraries (D-085 note):
# the tier runs it as a test only, nothing is distributed.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin petsc
img="${AKANTU_RS_PETSC_IMAGE:-akantu-rs-petsc:dev}"
if [ ! -x "$CI_REPO/scripts/petsc/run-petsc-tests.sh" ]; then
  ci_na petsc "this tree has no scripts/petsc/run-petsc-tests.sh (PETSc not merged)"
elif ! ci_have docker || ! docker image inspect "$img" > /dev/null 2>&1; then
  ci_skip petsc "Docker or image $img not available (ci/github/images.sh <cache> mpi petsc builds it from docker/petsc)"
else
  ci_step petsc --host -- env AKANTU_RS_MPI_IMAGE="$img" AKANTU_RS_MPI_TARGET="/target/$CI_TARGET_NAME-petsc" "$CI_REPO/scripts/petsc/run-petsc-tests.sh" "$@"
fi
ci_tier_end
