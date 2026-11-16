#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# usage: mpi.sh NP BINARY ARGS...   run BINARY on NP ranks in the Scotch image
# (ranks bound to cores 0..NP-1), ~/w/suite-setup mounted at the same path.
np="$1"; shift
exec docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -e SCOTCH_PTHREAD_NUMBER=1 -e AKANTU_SUITE_COMMIT \
  --cpuset-cpus "${AKANTU_JOB_CPUS:-0-7}" --memory 28g --memory-swap 28g --shm-size 4g --network none \
  --name "suite-setup-mpi-$$" \
  -v "$HOME/w/suite-setup:$HOME/w/suite-setup" -w "$HOME/w/suite-setup" \
  akantu-rs-scotch:dev \
  mpirun -n "$np" --bind-to core --map-by core "$@"
