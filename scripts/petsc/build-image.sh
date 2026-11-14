#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Build the akantu-rs PETSc test image (docker/petsc/Dockerfile) on top of
# akantu-rs-mpi:dev and record its package manifest, PETSc configuration,
# BLAS/LAPACK alternatives and image id under docker/petsc/. amd64 only
# (D-043).
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
image="${AKANTU_RS_PETSC_IMAGE:-akantu-rs-petsc:dev}"
base="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
docker build --memory 6g --build-arg BASE_IMAGE="$base" -t "$image" "$here/docker/petsc"
docker run --rm "$image" cat /usr/share/akantu-rs-petsc-packages.tsv \
  > "$here/docker/petsc/packages.tsv"
docker run --rm "$image" cat /usr/share/akantu-rs-petsc-configure.txt \
  > "$here/docker/petsc/petsc-configure.txt"
docker run --rm "$image" cat /usr/share/akantu-rs-petsc-alternatives.txt \
  > "$here/docker/petsc/alternatives.txt"
docker image inspect "$image" --format '{{.Id}}' > "$here/docker/petsc/image-id.txt"
echo "built $image: $(cat "$here/docker/petsc/image-id.txt")"
grep -E '^(libpetsc-real3\.22|libpetsc-real3\.22-dev|petsc-dev|libopenblas0-pthread|libopenmpi40|libmumps-5)' \
  "$here/docker/petsc/packages.tsv" || true
