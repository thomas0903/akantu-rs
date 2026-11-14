#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (EPFL/LSMS Akantu port). Clean-room comparison
# (D-030: only RESULTS of the compiled std::complex<double> division are
# observed; no libgcc source is read and no libgcc code is disassembled).
# Compares the division of the oracle image on this
# machine with the clean-room models on the 2e7 + 18^4 inputs of E25
# (cdiv_compare.cc): the aarch64 model v5, the x86 analysis's unfused model (every
# std::fma(x, y, z) of model_v1.h replaced by x*y + z, as cdiv-unfused.sh),
# the later model_v6 (both Baudin & Smith forms) when model_v6.h is in
# oracle/probes/dense/complex_cleanroom/ or given as CDIV_MODEL_V6=<path>,
# and the unfused model with other upscaling guards; prints every residual
# case of the unfused model.
# Usage: oracle/x86/fp-probe/cdiv-compare.sh > report.txt
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # pin + x86-64-v3 check
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
cr="${repo}/oracle/probes/dense/complex_cleanroom"
v6="${CDIV_MODEL_V6:-${cr}/model_v6.h}"
v6_mount=()
if [ -f "${v6}" ]; then v6_mount=(-v "$(cd "$(dirname "${v6}")" && pwd)/$(basename "${v6}"):/v6/model_v6.h:ro"); fi
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=2g --network none \
  -v "${cr}:/src:ro" -v "${here}:/diag:ro" ${v6_mount[@]+"${v6_mount[@]}"} akantu-oracle:dev bash -euc '
    mkdir -p /tmp/c && cp /src/*.h /diag/cdiv_compare.cc /tmp/c/ && cd /tmp/c
    sed -E "s/std::fma\(([^,()]+), ([^,()]+), ([^,()]+)\)/((\1) * (\2) + (\3))/g; s/model_v1\(/model_v1u(/g" \
      model_v1.h > model_v1u.h
    test "$(grep -c "std::fma" model_v1u.h)" = 0
    sed -E "s/model_v1\.h/model_v1u.h/; s/model_v1\(/model_v1u(/g; s/model_v5\(/model_v5u(/g; s/absmax/absmax_u/g" \
      model_v5.h > model_v5u.h
    def=""
    if [ -f /v6/model_v6.h ]; then cp /v6/model_v6.h . && def=-DHAVE_V6; fi
    echo "# $(g++ --version | head -1); debian $(cat /etc/debian_version) $(uname -m); model_v6: ${def:-absent}"
    echo "# host: $(grep -m1 "model name" /proc/cpuinfo | cut -d: -f2 | sed "s/^ //")"
    g++ -O2 -std=c++20 -ffp-contract=off $def cdiv_compare.cc -o cdiv_compare
    ./cdiv_compare'
