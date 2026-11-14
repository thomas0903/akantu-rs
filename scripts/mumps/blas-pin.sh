# SPDX-License-Identifier: LGPL-3.0-or-later
# shellcheck shell=bash
# BLAS pin of every MUMPS run (D-050), sourced by
# the MUMPS scripts on the host and inside the containers. OpenBLAS
# (DYNAMIC_ARCH) selects its kernels from the run-time CPU unless
# OPENBLAS_CORETYPE pins them, and MUMPS's results depend on that choice:
# 1-rank solves are bitwise reproducible (run to run, host to host, port vs
# oracle) only for the same platform, shared objects, core type and thread
# count.
#   x86_64  -> Haswell (x86-64-v3: AVX2 + FMA; every x86_64 measurement host, Intel
#              >= 2013, AMD Zen), the pin of the x86_64 fixture set
#              fixtures/solver/mumps/x86_64-haswell;
#   aarch64 -> unset: the aarch64 platform variant (fixtures/solver/mumps)
#              was generated with OpenBLAS's run-time core; no aarch64 pin
#              is verified yet.
# Threads: always 1. Also set as ENV in docker/mumps/Dockerfile; scripts pass
# both values explicitly into every container they start, so the pin cannot
# be lost.
akantu_blas_coretype() {
  case "$(uname -m)" in
    x86_64) echo Haswell ;;
    *) echo "" ;;
  esac
}
AKANTU_BLAS_CORETYPE="$(akantu_blas_coretype)"
AKANTU_BLAS_THREADS=1
# Docker arguments that pass the pin into a container.
# (An empty OPENBLAS_CORETYPE is never passed: OpenBLAS would treat it as an
# unknown core and fall back to a generic one.)
akantu_blas_docker_env() {
  if [ -n "${AKANTU_BLAS_CORETYPE}" ]; then
    printf '%s\n' -e "OPENBLAS_CORETYPE=${AKANTU_BLAS_CORETYPE}"
  fi
  printf '%s\n' -e "OPENBLAS_NUM_THREADS=${AKANTU_BLAS_THREADS}" -e "OMP_NUM_THREADS=1"
}
# Export the pin into the current shell (inside a container).
akantu_blas_export() {
  if [ -n "${AKANTU_BLAS_CORETYPE}" ]; then
    export OPENBLAS_CORETYPE="${AKANTU_BLAS_CORETYPE}"
  else
    unset OPENBLAS_CORETYPE
  fi
  export OPENBLAS_NUM_THREADS="${AKANTU_BLAS_THREADS}" OMP_NUM_THREADS=1
}
