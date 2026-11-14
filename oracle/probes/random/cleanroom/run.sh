#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs. Builds and runs the clean-room black-box experiments of the
# random module (D-030, crates/akantu-core/src/random/CLEANROOM_LOG.md) inside
# the oracle image and refreshes the edge-case fixture
# crates/akantu-core/tests/fixtures/random/cleanroom_edges.json.
#
# Only the public <random> API is used; no library header is read.
#
# Usage: oracle/probes/random/cleanroom/run.sh [--no-fixture]
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
write_fixture=1
[[ "${1:-}" == "--no-fixture" ]] && write_fixture=0

out="$repo/target/cleanroom-random"
mkdir -p "$out"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory=2g -v "$here":/src:ro -v "$out":/out -w /out akantu-oracle:dev \
  bash -euo pipefail -c '
    flags="-std=c++20 -O2 -ffp-contract=off -Wall -Wextra"
    g++ $flags -o exp_canonical /src/exp_canonical.cc && ./exp_canonical > exp_canonical.txt
    g++ $flags -o exp_canonical_k /src/exp_canonical_k.cc && ./exp_canonical_k > exp_canonical_k.txt
    g++ $flags -D_GLIBCXX_ASSERTIONS -o exp_assertions /src/exp_assertions.cc \
      && ./exp_assertions > exp_assertions.txt
    g++ $flags -o exp_gamma_boundary /src/exp_gamma_boundary.cc \
      && ./exp_gamma_boundary > exp_gamma_boundary.txt
    g++ $flags -o exp_invalid /src/exp_invalid.cc && ./exp_invalid > exp_invalid.txt
    g++ $flags -o edges /src/edges.cc && ./edges > cleanroom_edges.json
    g++ --version | head -1 > toolchain.txt
    ldd --version | head -1 >> toolchain.txt
    uname -m >> toolchain.txt
  '
cat "$out/toolchain.txt"
echo "experiment outputs in $out"
if [[ $write_fixture == 1 ]]; then
  cp "$out/cleanroom_edges.json" "$repo/crates/akantu-core/tests/fixtures/random/cleanroom_edges.json"
  echo "fixture written: crates/akantu-core/tests/fixtures/random/cleanroom_edges.json"
fi
