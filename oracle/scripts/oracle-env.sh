# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (oracle tooling). Sourced (not executed) by every script
# that starts an oracle container; macOS bash 3.2 compatible.
#
# ORACLE_ENV_ARGS: `docker run` arguments that pin the numerical environment
# of the x86_64 oracle (D-050 item 5): OpenBLAS's DYNAMIC_ARCH kernel
# set (OPENBLAS_CORETYPE=Haswell; otherwise chosen from the host CPU at run
# time, which changes MUMPS/PETSc results) and single-threaded
# BLAS/OpenMP. The amd64 image sets the same ENV; passing it explicitly keeps
# the pin even with another image tag. Empty on arm64 (recorded aarch64
# oracle unchanged). Use as: ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"}
# (bash 3.2 + set -u safe). A diagnostic that overrides the pin passes its
# own -e after these arguments (the last -e wins).
#
# Host check (D-073 (2)): the authoritative x86_64 oracle
# runs on x86-64-v3 hosts only. The `Haswell` OpenBLAS kernels need AVX2 +
# FMA (a kernel set the CPU cannot execute dies with SIGILL), and glibc's
# libm selects other ifunc variants of exp/log/pow/sin/cos/tan... without
# FMA/AVX2 (27 of the 604 random `libm.json` leaves change when FMA/AVX2 are
# masked). On amd64 this file therefore refuses to continue unless
# the CPU that runs the containers (the Docker host, i.e. this machine's
# /proc/cpuinfo) has every x86-64-v3 feature: avx avx2 bmi1 bmi2 f16c fma
# movbe xsave and abm (LZCNT). ORACLE_ALLOW_NON_V3=1 lets a diagnostic run
# continue on such a host; its fixtures are not authoritative, and
# build_env.py records the measured level (`x86_64_level`) in every manifest
# either way. ORACLE_X86_64_LEVEL is exported for the scripts.
case "$(docker version --format '{{.Server.Arch}}' 2>/dev/null || uname -m)" in
  amd64|x86_64)
    ORACLE_ARCH=amd64
    ORACLE_ENV_ARGS=(-e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1)
    ;;
  *)
    ORACLE_ARCH=arm64
    ORACLE_ENV_ARGS=()
    ;;
esac

# Prints "v3" when the host CPU has every x86-64-v3 feature, otherwise
# "below-v3 (missing: <features>)", or "unknown (...)" without /proc/cpuinfo.
# ORACLE_CPUINFO overrides the file (self-tests only).
oracle_x86_64_level() {
  local cpuinfo="${ORACLE_CPUINFO:-/proc/cpuinfo}" flags missing f
  if [ ! -r "${cpuinfo}" ]; then
    echo "unknown (${cpuinfo} not readable)"
    return 0
  fi
  flags=" $(grep -m1 '^flags' "${cpuinfo}" | cut -d: -f2 || true) "
  missing=""
  for f in avx avx2 bmi1 bmi2 f16c fma movbe xsave abm; do
    case "${flags}" in
      *" ${f} "*) ;;
      *) missing="${missing} ${f}" ;;
    esac
  done
  if [ -z "${missing}" ]; then
    echo "v3"
  else
    echo "below-v3 (missing:${missing})"
  fi
}

if [ "${ORACLE_ARCH}" = amd64 ]; then
  ORACLE_X86_64_LEVEL="$(oracle_x86_64_level)"
  export ORACLE_X86_64_LEVEL
  if [ "${ORACLE_X86_64_LEVEL}" != v3 ]; then
    if [ "${ORACLE_ALLOW_NON_V3:-0}" = 1 ]; then
      echo "WARNING: host CPU is not x86-64-v3 (${ORACLE_X86_64_LEVEL}); ORACLE_ALLOW_NON_V3=1: diagnostic run, fixtures NOT authoritative (D-073 (2))" >&2
    else
      echo "ERROR: the x86_64 oracle needs an x86-64-v3 host (AVX2, FMA, BMI1/2, F16C, MOVBE, LZCNT): ${ORACLE_X86_64_LEVEL}." >&2
      echo "       OpenBLAS's pinned Haswell kernels need AVX2 + FMA and glibc's libm would select other variants (D-073 (2))." >&2
      echo "       Use another host; ORACLE_ALLOW_NON_V3=1 only for a non-authoritative diagnostic." >&2
      exit 1
    fi
  fi
fi
