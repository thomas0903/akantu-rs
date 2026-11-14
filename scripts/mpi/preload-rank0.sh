#!/bin/sh
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs MPI test support (no upstream counterpart): run "$@" with
# LD_PRELOAD="$AKANTU_TEST_PRELOAD" on every rank and, on rank 0 only,
# "$AKANTU_TEST_PRELOAD_RANK0" appended (an adversarial
# PMIx_Abort interposer on one rank only, the other ranks making the real
# request). Usage, under mpirun:
#   mpirun -x AKANTU_TEST_PRELOAD=abort_shim.so \
#          -x AKANTU_TEST_PRELOAD_RANK0=pmix_shim_block.so \
#          -np N sh scripts/mpi/preload-rank0.sh <binary> <args...>
# It stands in for the MPMD form
#   mpirun -np 1 -x LD_PRELOAD=a:b prog : -np N-1 -x LD_PRELOAD=a prog,
# which the MPI image's launcher cannot start: with Open MPI 5.0.7-1 /
# PRRTE 3.0.8 (Debian trixie) every MPMD job, colon syntax or --app file,
# even `mpirun -np 1 hostname : -np 1 hostname`, dumps core at once
# (docs/TESTING.md).
rank="${OMPI_COMM_WORLD_RANK:-${PMIX_RANK:-}}"
if [ -z "$rank" ]; then
  echo "preload-rank0.sh: no OMPI_COMM_WORLD_RANK / PMIX_RANK (not under mpirun?)" >&2
  exit 2
fi
LD_PRELOAD="${AKANTU_TEST_PRELOAD:-}"
if [ "$rank" = 0 ] && [ -n "${AKANTU_TEST_PRELOAD_RANK0:-}" ]; then
  LD_PRELOAD="${LD_PRELOAD:+$LD_PRELOAD:}$AKANTU_TEST_PRELOAD_RANK0"
fi
export LD_PRELOAD
exec "$@"
