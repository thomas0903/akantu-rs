#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (EPFL/LSMS Akantu port). Root-cause probe: build the
# dense probe (oracle/probes/dense/probe.cc) like run.sh's strict variant, with
# Eigen's cache sizes forced to the aarch64 defaults (eigen_cache_shim.cc) and
# with __divdc3 replaced by the clean-room aarch64 model (divdc3_model.cc,
# `model`) or libgcc's own (`libgcc`); prints the records.
# Usage: oracle/x86/fp-probe/dense-divdc3.sh <model|libgcc> > records.txt
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
variant="${1:?model|libgcc}"
case "${variant}" in model|libgcc) ;; *) echo "unknown variant ${variant}" >&2; exit 2 ;; esac
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=4g --network none \
  -v akantu-oracle-build:/build:ro -v "${repo}/oracle/probes/dense:/src:ro" -v "${here}:/diag:ro" \
  -e EIGEN_CACHE_SIZES=16384,524288,524288 -e V="${variant}" akantu-oracle:dev bash -euc '
    inst=/build/install-strict
    extra=""
    [ "$V" = model ] && extra="/diag/divdc3_model.cc -I/src/complex_cleanroom"
    g++ -std=c++20 -O2 -DNDEBUG -I/usr/include/eigen3 -DOMPI_SKIP_MPICXX \
      -ffp-contract=off -DEIGEN_DONT_VECTORIZE \
      -I$inst/include/akantu -I$inst/include/akantu_iterators -I$inst/include \
      /src/probe.cc /diag/eigen_cache_shim.cc $extra -o /tmp/probe -L$inst/lib -Wl,-rpath,$inst/lib -lakantu
    nm /tmp/probe | grep -w __divdc3 >&2
    /tmp/probe'
