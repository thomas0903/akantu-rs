#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (EPFL/LSMS Akantu port). Root-cause probe, clean
# room (D-030): only RESULTS of the compiled std::complex<double> division are
# observed; no libgcc source is read and no libgcc code is disassembled.
# Runs the clean-room E25 random/grid comparison (oracle/probes/dense/
# complex_cleanroom/e25_special_random.cc) on this machine against
#   fused   : model_v5 as derived on aarch64 (std::fma in the Smith core), and
#   unfused : the same model with every std::fma(x, y, z) replaced by x*y + z
#             (built with -ffp-contract=off, so nothing is re-fused),
# to test whether the x86_64 libgcc division is the unfused model.
# Usage: oracle/x86/fp-probe/cdiv-unfused.sh   (prints both reports)
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=2g --network none \
  -v "${repo}/oracle/probes/dense/complex_cleanroom:/src:ro" akantu-oracle:dev bash -euc '
    mkdir -p /tmp/u && cp /src/*.h /src/e25_special_random.cc /tmp/u/ && cd /tmp/u
    sed -E "s/std::fma\(([^,()]+), ([^,()]+), ([^,()]+)\)/((\1) * (\2) + (\3))/g" model_v1.h > model_v1u.h
    test "$(grep -c "std::fma" model_v1u.h)" = 0
    sed "s/model_v1\.h/model_v1u.h/" model_v5.h > model_v5u.h
    sed "s/model_v5\.h/model_v5u.h/" e25_special_random.cc > e25u.cc
    cxx="g++ -O2 -std=c++20 -ffp-contract=off"
    echo "== fused model_v5 (aarch64-derived) vs $(uname -m) division"; $cxx e25_special_random.cc -o e25 && ./e25 | tail -2
    echo "== unfused model_v5 vs $(uname -m) division"; $cxx e25u.cc -o e25u && ./e25u | tail -2
    echo "== first unfused mismatches (if any)"; ./e25u | grep "^MIS" | head -5 || true'
