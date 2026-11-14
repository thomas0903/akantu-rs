#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Build the akantu-rs MPI test image (docker/mpi/Dockerfile) and record its
# package manifest in docker/mpi/packages.tsv and its id in
# docker/mpi/image-id.txt. One heavy build at a time; memory-limited.
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
image="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
docker build --memory 6g -t "$image" "$here/docker/mpi"
docker run --rm "$image" cat /usr/share/akantu-rs-mpi-packages.tsv \
  > "$here/docker/mpi/packages.tsv"
docker image inspect "$image" --format '{{.Id}}' > "$here/docker/mpi/image-id.txt"
echo "built $image: $(cat "$here/docker/mpi/image-id.txt")"
grep -E '^(openmpi-bin|libopenmpi-dev|libclang-dev|python3-mpi4py)\b' "$here/docker/mpi/packages.tsv"
