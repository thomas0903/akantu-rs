#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (EPFL/LSMS Akantu port). Root-cause probe: build the
# dense probes (oracle/probes/dense/{probe,gemm_probe}.cc) exactly like
# run.sh/run_gemm.sh (strict flags, install-strict) but with
# eigen_cache_shim.cc LINKED INTO the executable (the probes instantiate Eigen
# themselves and an executable binds its own copy of Eigen's cache-size static,
# so LD_PRELOAD cannot reach it), and print the records with Eigen's cache
# sizes forced to <l1,l2,l3> or left to the CPUID query (cpuid).
# Usage: oracle/x86/fp-probe/eigen-cache-dense.sh <probe|gemm_probe> <l1,l2,l3|cpuid> > records.txt
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
probe="${1:?probe|gemm_probe}"; caches="${2:?l1,l2,l3 or cpuid}"
case "${probe}" in probe|gemm_probe) ;; *) echo "unknown probe ${probe}" >&2; exit 2 ;; esac
env_caches=()
[ "${caches}" = cpuid ] || env_caches=(-e EIGEN_CACHE_SIZES="${caches}")
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=4g --network none -v akantu-oracle-build:/build:ro \
  -v "${repo}/oracle/probes/dense:/src:ro" -v "${here}:/diag:ro" \
  "${env_caches[@]+"${env_caches[@]}"}" -e EIGEN_CACHE_SIZES_REPORT=1 -e P="${probe}" \
  akantu-oracle:dev bash -euc '
    inst=/build/install-strict
    g++ -std=c++20 -O2 -DNDEBUG -I/usr/include/eigen3 -DOMPI_SKIP_MPICXX \
      -ffp-contract=off -DEIGEN_DONT_VECTORIZE \
      -I$inst/include/akantu -I$inst/include/akantu_iterators -I$inst/include \
      /src/$P.cc /diag/eigen_cache_shim.cc -o /tmp/$P -L$inst/lib -Wl,-rpath,$inst/lib -lakantu
    /tmp/$P'
