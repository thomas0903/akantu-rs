#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-010, CI-020: valgrind job and sanitizer suppressions, U8).
# memcheck: valgrind memcheck of the FFI `unsafe` code (MPI, MUMPS, Scotch, libm, C ABI, PETSc) in the project images
#
# CI_MEMCHECK_KIND = mpi (default) | mumps | petsc selects the base image
# akantu-rs-<kind>:dev (mpi: Open MPI, libm, C ABI; mumps: + MUMPS, Scotch,
# PT-Scotch; petsc: + PETSc). The tier builds `akantu-rs-memcheck-<kind>:dev`
# FROM it with docker/memcheck/Dockerfile (valgrind + libc6-dbg) and runs
# ci/memcheck/run-in-image.sh <kind> in it through scripts/mpi/in-docker.sh
# (which acceptance binaries run, the verdict rule and the suppression file:
# that script's header). Why valgrind and not AddressSanitizer: ASan needs
# nightly + `-Zbuild-std` and an instrumented Open MPI / MUMPS / PETSc to see
# anything across the FFI boundary, while memcheck runs the unmodified
# binaries and the images' system libraries (the oracle valgrind recipe,
# docs/ORACLE_BUILD.md, used the same way). Miri (tier `miri`) covers
# the safe-wrapper code that memcheck cannot see into; this tier covers what
# Miri cannot run (FFI). Always Docker (the backend choice does not apply).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin memcheck
kind="${CI_MEMCHECK_KIND:-mpi}"
case "$kind" in mpi | mumps | petsc) ;; *)
  ci_die "CI_MEMCHECK_KIND must be mpi, mumps or petsc (got '$kind')"
  ;;
esac
base="${CI_MEMCHECK_BASE_IMAGE:-akantu-rs-$kind:dev}"
img="akantu-rs-memcheck-$kind:dev"
if ! ci_have docker || ! docker image inspect "$base" > /dev/null 2>&1; then
  ci_skip "memcheck-$kind" "Docker or image $base not available (ci/github/images.sh builds it from docker/)"
else
  ci_step "image-$kind" --host -- docker build -q --build-arg "BASE_IMAGE=$base" -t "$img" "$CI_REPO/docker/memcheck"
  if [ "$(ci_step_status "image-$kind")" = pass ] || [ "$CI_DRY_RUN" = 1 ]; then
    ci_step "memcheck-$kind" --host --timeout 10800 -- env \
      AKANTU_RS_MPI_IMAGE="$img" AKANTU_RS_MPI_TARGET="/target/$CI_TARGET_NAME-memcheck-$kind" \
      AKANTU_RS_MPI_IMAGE_ID="$(docker image inspect "$img" --format '{{.Id}}' 2> /dev/null || echo unknown)" \
      AKANTU_RS_GIT_HEAD="$(git -C "$CI_REPO" rev-parse HEAD 2> /dev/null || echo unknown)" \
      "$CI_REPO/scripts/mpi/in-docker.sh" bash /src/ci/memcheck/run-in-image.sh "$kind"
  else
    ci_skip "memcheck-$kind" "the memcheck image did not build"
  fi
fi
ci_tier_end
