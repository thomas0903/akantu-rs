#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-016: image builds).
# Workflow helper: load the project images from a cache directory,
# or build them from docker/ and save them there (for actions/cache).
#
#   ci/github/images.sh <cache-dir> [mpi] [scotch] [mumps] [petsc]   # default: mpi scotch mumps
#
# Plain `docker build` (not scripts/mpi/build-image.sh, which rewrites the
# committed docker/mpi/packages.tsv and image-id.txt). The image ids are
# printed; they differ from one build to the next (apt is not version-pinned,
# D-034) and from the ids recorded in docker/*/image-id.txt, so each run keeps
# them in provenance.txt (see ci/github/README.md, "Images").
set -euo pipefail
cache="${1:?usage: images.sh <cache-dir> [mpi] [scotch] [mumps] [petsc]}"
shift
want="${*:-mpi scotch mumps}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$cache"
tar="$cache/akantu-rs-images.tar"
if [ -s "$tar" ]; then
  docker load -i "$tar"
fi
built=0
for w in $want; do
  case "$w" in
    mpi) tag=akantu-rs-mpi:dev dir=docker/mpi ;;
    scotch) tag=akantu-rs-scotch:dev dir=docker/scotch ;;
    mumps) tag=akantu-rs-mumps:dev dir=docker/mumps ;;
    petsc) tag=akantu-rs-petsc:dev dir=docker/petsc ;;
    *)
      echo "images.sh: unknown image '$w'" >&2
      exit 2
      ;;
  esac
  if ! docker image inspect "$tag" > /dev/null 2>&1; then
    if [ "$w" != mpi ] && ! docker image inspect akantu-rs-mpi:dev > /dev/null 2>&1; then
      docker build -t akantu-rs-mpi:dev "$repo/docker/mpi"
    fi
    docker build -t "$tag" "$repo/$dir"
    built=1
  fi
done
if [ "$built" = 1 ]; then
  # shellcheck disable=SC2046
  docker save -o "$tar" $(docker images --format '{{.Repository}}:{{.Tag}}' | grep '^akantu-rs-' || true)
fi
docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | grep '^akantu-rs-' || true
