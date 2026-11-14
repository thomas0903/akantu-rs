#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the oracle image (host wrapper: scripts/mumps/oracle-compare.sh).
# Builds the probe in /tmp against /build/install-<config> (read-only volume)
# and solves every exported system with upstream SparseSolverMumps on 1/2/4
# ranks, ORACLE_RUNS times (default 2; tags oracle, oracle2, oracle3:
# run-to-run variation, D-035), plus the UD-216 retry path on elasticity_6
# with the ICNTL(14) interposer (tags oracle-retry, oracle-retry2, …).
# Exchange directory: /x/in (systems from the Rust binary), /x/out (results).
# BLAS pin (D-050): OPENBLAS_CORETYPE / OPENBLAS_NUM_THREADS are
# taken from the caller's environment (scripts/mumps/oracle-compare.sh passes
# scripts/mumps/blas-pin.sh's values) and recorded in the provenance with the
# core OpenBLAS actually reports.
set -euo pipefail
config="${1:-strict}"
ranks="${2:-1 2 4}"
src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bld="/tmp/mumps-probe-${config}"
. "/build/install-${config}/share/akantu-oracle/env.sh"
cmake -S "${src}" -B "${bld}" -G Ninja -DORACLE_CONFIG="${config}" > /x/out/cmake.log 2>&1
cmake --build "${bld}" -j 2 >> /x/out/cmake.log 2>&1
export SCOTCH_PTHREAD_NUMBER=1 AKA_SEED=1 OPENBLAS_NUM_THREADS="${OPENBLAS_NUM_THREADS:-1}" OMP_NUM_THREADS=1
blasx=(-x OPENBLAS_NUM_THREADS)
if [ -n "${OPENBLAS_CORETYPE:-}" ]; then export OPENBLAS_CORETYPE; blasx+=(-x OPENBLAS_CORETYPE); fi
runs="${ORACLE_RUNS:-2}"
suffix() { [ "$1" = 1 ] && echo "" || echo "$1"; }
systems=$(ls /x/in/*.p1.r0.txt | xargs -n1 basename | sed 's/\.p1\.r0\.txt$//' | sort)
for p in ${ranks}; do
  for k in $(seq 1 "${runs}"); do
    tag="oracle$(suffix "$k")"
    mpiexec --oversubscribe --bind-to none "${blasx[@]}" -n "${p}" "${bld}/mumps_probe" /x/in /x/out "${tag}" \
      ${systems} > "/x/out/probe-${tag}-p${p}.log" 2>&1
    # UD-216: upstream's retry path (ICNTL(14) lowered to -80 before the
    # first analysis; upstream then retries with -70, -60, ... like the Rust
    # test)
    tag="oracle-retry$(suffix "$k")"
    AKANTU_ICNTL14=-80 AKANTU_ICNTL14_LOG=1 LD_PRELOAD="${bld}/libicntl14_interposer.so" \
      mpiexec --oversubscribe --bind-to none "${blasx[@]}" -x AKANTU_ICNTL14 -x AKANTU_ICNTL14_LOG -x LD_PRELOAD \
      -n "${p}" "${bld}/mumps_probe" /x/in /x/out "${tag}" elasticity_6 \
      > "/x/out/probe-${tag}-p${p}.log" 2>&1
  done
done
# the core OpenBLAS actually runs (its banner with OPENBLAS_VERBOSE=2)
mkdir -p /tmp/mumps-blas-core
core=$(OPENBLAS_VERBOSE=2 mpiexec --oversubscribe --bind-to none "${blasx[@]}" -x OPENBLAS_VERBOSE -n 1 \
         "${bld}/mumps_probe" /x/in /tmp/mumps-blas-core verbose upstream_diagonal 2>&1 \
       | sed -n 's/^Core: *//p' | head -1)
triplet="$(uname -m)-linux-gnu"
sha() { local f; f=$(readlink -f "$1" 2>/dev/null) && [ -f "$f" ] && echo "$f $(sha256sum "$f" | cut -d' ' -f1)" || echo "missing $1"; }
lib=$(readlink -f "/build/install-${config}/lib/libakantu.so")
{
  echo "config=${config}"
  echo "image=${ORACLE_IMAGE_ID:-unknown}"
  echo "libakantu=${lib} $(sha256sum "${lib}" | cut -d' ' -f1)"
  echo "platform=$(uname -m)"
  echo "libdmumps=$(sha "/usr/lib/${triplet}/libdmumps.so")"
  echo "libmumps_common=$(sha "/usr/lib/${triplet}/libmumps_common.so")"
  echo "libblas=$(sha "/usr/lib/${triplet}/libblas.so.3")"
  echo "libopenblas=$(sha "/usr/lib/${triplet}/libopenblas.so.0")"
  echo "libscalapack=$(sha "/usr/lib/${triplet}/libscalapack-openmpi.so")"
  echo "OPENBLAS_CORETYPE=${OPENBLAS_CORETYPE:-<unset: run-time selection>}"
  echo "OPENBLAS_NUM_THREADS=${OPENBLAS_NUM_THREADS}"
  echo "openblas_core_reported=${core:-unknown}"
  echo "runs=${runs}"
  echo "cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
  echo "probe_sha256=$(cat "${src}/probe.cc" "${src}/icntl14_interposer.c" | sha256sum | cut -d' ' -f1)"
} > /x/out/oracle-provenance.txt
echo "oracle probe done"
