#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (EPFL/LSMS Akantu port). Root-cause probe: re-run
# a model/solver probe (stage only) with Eigen's cache sizes forced through the
# LD_PRELOAD shim eigen_cache_shim.cc, to test whether x86_64 vs aarch64
# differences come from Eigen's CPUID cache-size query (Eigen 3.4.0 uses
# CPUID on x86_64 and the defaults 16 KiB / 512 KiB / 512 KiB on aarch64).
# Explicit <l1,l2,l3> sizes work on the pinned install as well: Eigen keeps the
# sizes in one process-wide static that libakantu.so exports, and the shim
# sets it before any module reads it; only `cpuid` is
# meaningless on a pinned install (no oracle binary queries CPUID). The probes
# would refuse to write fixtures with non-pinned sizes (akantu_probe/
# eigen_pin.hh), so this diagnostic sets AKANTU_PROBE_ALLOW_UNPINNED_EIGEN=1;
# its manifests record the measured sizes (eigen_cache_sizes_measured).
# Usage: oracle/x86/fp-probe/eigen-cache-run.sh <probe> <out-dir> <l1,l2,l3|cpuid> [<case>[,<case>...]]
#   <probe>: smm | smm_fd | solver | solver_model   (cases: smm/smm_fd/solver_model only)
# Env: AKANTU_SRC_HOST (reference checkout).
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
probe="${1:?probe}"; out="${2:?out dir}"; caches="${3:?l1,l2,l3 or cpuid}"; cases="${4:-}"
case "${probe}" in
  smm) dir=smm; prefix=SMM; extra=(-e SMM_PROBE_BIN=smm_probe) ;;
  smm_fd) dir=smm; prefix=SMM; extra=(-e SMM_PROBE_BIN=smm_fd_probe) ;;
  solver) dir=solver; prefix=SOLVER; extra=() ;;
  solver_model) dir=solver_model; prefix=SOLVER_MODEL; extra=() ;;
  *) echo "unknown probe ${probe}" >&2; exit 2 ;;
esac
[ -z "${cases}" ] || extra+=(-e "${prefix}_PROBE_CASES=${cases}")
[ "${caches}" = cpuid ] || extra+=(-e EIGEN_CACHE_SIZES="${caches}")
mkdir -p "${out}"; out="$(cd "${out}" && pwd)"
image_id="$(docker image inspect --format '{{.Id}}' akantu-oracle:dev)"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=4g --memory-swap=4g --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST:?}:/akantu-src:ro" -v akantu-oracle-build:/build:ro \
  -v "${repo}/oracle/probes:/probes:ro" -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${here}:/diag:ro" -v "${out}:/fixtures" \
  -e AKANTU_PROBE_REFERENCE_SHA=35a851780d0b9471d258ed53a003ae98aaa95ea0 \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE=diagnostic -e AKANTU_PROBE_CREATED=diagnostic \
  -e AKANTU_PROBE_COMMAND="oracle/x86/fp-probe/eigen-cache-run.sh ${probe} ${caches} ${cases}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="$(git -C "${repo}" rev-parse HEAD)" \
  -e ORACLE_IMAGE_ID="${image_id}" -e "${prefix}_PROBE_STAGE_ONLY=1" \
  "${extra[@]}" -e EIGEN_CACHE_SIZES_REPORT=1 -e AKANTU_PROBE_ALLOW_UNPINNED_EIGEN=1 \
  -e DIR="${dir}" akantu-oracle:dev bash -c '
    set -e
    g++ -O2 -fPIC -shared -std=c++20 -ffp-contract=off -DEIGEN_DONT_VECTORIZE \
      -I/usr/include/eigen3 /diag/eigen_cache_shim.cc -o /tmp/libeigen_cache_shim.so
    export LD_PRELOAD=/tmp/libeigen_cache_shim.so
    bash "/probes/${DIR}/run-in-container.sh" strict'
